use serde::Serialize;

use std::{
    collections::HashSet,
    fs::{self, File},
    path::PathBuf,
};

use libyaml::{self, Encoding, Event, ParserIter};

use clap::Parser;

use anyhow::{bail, Result};

/// A migration tool to "normalize" the liblouis yaml test files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The yaml file to convert
    yaml: PathBuf,
    /// Write output to FILE instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum TestMode {
    #[default]
    Forward,
    Backward,
    BothDirections,
    Display,
    Hyphenate,
    HyphenateBraille,
}

#[derive(Debug, Default, Serialize)]
struct Table {
    language: String,
    grade: u8,
    system: String,
    path: PathBuf,
}

#[derive(Debug, Default, Serialize)]
pub struct TestSuite {
    display_table: PathBuf,
    table: Table,
    mode: TestMode,
    tests: Vec<Test>,
}

#[derive(Debug, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    NoContractions,
    CompbrlAtCursor,
    DotsIo,
    CompbrlLeftCursor,
    UcBrl,
    NoUndefined,
    PartialTrans,
}

fn is_false(b: &bool) -> bool {
    !(*b)
}

#[derive(Debug, Default, Serialize)]
pub struct Test {
    input: String,
    expected: String,
    #[serde(skip_serializing_if = "is_false")]
    xfail: bool,
    //    typeform:
    #[serde(skip_serializing_if = "Vec::is_empty")]
    input_pos: Vec<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    output_pos: Vec<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor_pos: Option<u16>,
    #[serde(skip_serializing_if = "HashSet::is_empty")]
    mode: HashSet<Mode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_length: Option<u16>,
}

fn read_stream_start(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::StreamStart { encoding })) => match encoding {
            Some(Encoding::Utf8) => Ok(()),
            _ => bail!("Encoding {:?} not supported", encoding),
        },
        _ => bail!("Expected StreamStart"),
    }
}

fn read_stream_end(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::StreamEnd)) => Ok(()),
        _ => bail!("Expected StreamEnd"),
    }
}

fn read_document_start(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::DocumentStart { .. })) => Ok(()),
        _ => bail!("Expected DocumentStart"),
    }
}

fn read_document_end(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::DocumentEnd { .. })) => Ok(()),
        _ => bail!("Expected DocumentEnd"),
    }
}

fn read_mapping_start(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::MappingStart { .. })) => Ok(()),
        _ => bail!("Expected MappingStart"),
    }
}

fn read_mapping_end(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::MappingEnd)) => Ok(()),
        _ => bail!("Expected MappingEnd"),
    }
}

fn read_sequence_start(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::SequenceStart { .. })) => Ok(()),
        _ => bail!("Expected SequenceStart"),
    }
}

fn read_sequence_end(iter: &mut ParserIter) -> Result<()> {
    match iter.next() {
        Some(Ok(Event::SequenceEnd)) => Ok(()),
        _ => bail!("Expected SequenceEnd"),
    }
}

fn read_scalar(iter: &mut ParserIter) -> Result<String> {
    match iter.next() {
        Some(Ok(Event::Scalar { value, .. })) => Ok(value),
        _ => bail!("Expected Scalar"),
    }
}

fn parse_table(iter: &mut ParserIter) -> Result<Table> {
    read_mapping_start(iter)?;
    let mut table: Table = Default::default();
    while let Some(Ok(event)) = iter.next() {
        match event {
            Event::Scalar { value, .. } => {
                table = match value.as_str() {
                    "language" => Table {
                        language: read_scalar(iter)?,
                        ..table
                    },
                    "grade" => Table {
                        grade: read_scalar(iter)?.parse::<u8>()?,
                        ..table
                    },
                    "system" => Table {
                        system: read_scalar(iter)?,
                        ..table
                    },
                    "__assert-match" => Table {
                        path: read_scalar(iter)?.into(),
                        ..table
                    },
                    _ => bail!("Expected table attribute, got {:?}", value),
                };
            }
            Event::MappingEnd => {
                break;
            }
            _ => bail!("Expected Scalar or MappingEnd, got {:?}", event),
        };
    }
    Ok(table)
}

fn parse_flags(iter: &mut ParserIter) -> Result<TestMode> {
    read_mapping_start(iter)?;
    match iter.next() {
        Some(Ok(Event::Scalar { ref value, .. })) if value == "testmode" => match iter.next() {
            Some(Ok(Event::Scalar { value, .. })) => {
                let mode = match value.as_str() {
                    "backward" => TestMode::Backward,
                    "bothDirections" => TestMode::BothDirections,
                    "display" => TestMode::Display,
                    "hyphenate" => TestMode::Hyphenate,
                    "hyphenateBraille" => TestMode::HyphenateBraille,
                    _ => bail!("Testmode {:?} not supported", value),
                };
                read_mapping_end(iter)?;
                Ok(mode)
            }
            _ => bail!("Expected Scalar"),
        },
        _ => bail!("Expected Scalar testmode"),
    }
}

fn read_xfail_value(value: String) -> bool {
    match value.as_str() {
        "off" => false,
        "false" => false,
        _ => true,
    }
}

fn parse_xfail_value(iter: &mut ParserIter) -> Result<bool> {
    let xfail = match iter.next() {
        Some(Ok(Event::Scalar { value, .. })) => read_xfail_value(value),
        Some(Ok(Event::MappingStart { .. })) => {
            match read_scalar(iter)?.as_str() {
                "forward" => (),
                "backward" => (),
                other => bail!("Expected 'forward' or 'backward', got {:?}", other),
            };
            let xfail = read_xfail_value(read_scalar(iter)?);
            read_mapping_end(iter)?;
            xfail
        }
        other => bail!("Expected scalar xfail value, got {:?}", other),
    };
    Ok(xfail)
}

fn parse_test(iter: &mut ParserIter) -> Result<Test> {
    let input = read_scalar(iter)?;
    let expected = read_scalar(iter)?;
    match iter.next() {
        Some(Ok(Event::SequenceEnd)) => Ok(Test {
            input,
            expected,
            ..Default::default()
        }),
        Some(Ok(Event::MappingStart { .. })) => {
            let mut xfail = false;
            while let Some(Ok(event)) = iter.next() {
                match event {
                    Event::Scalar { ref value, .. } if value == "xfail" => {
                        xfail = parse_xfail_value(iter)?;
                    }
                    Event::MappingEnd => {
                        break;
                    }
                    _ => {
                        bail!("Expected Scalar or MappingEnd, got {:?}", event);
                    }
                }
            }

            read_sequence_end(iter)?;

            Ok(Test {
                input,
                expected,
                xfail,
                ..Default::default()
            })
            // handle options
        }
        _ => bail!("Expected SequenceEnd or MappingStart"),
    }
}

fn parse_tests(iter: &mut ParserIter) -> Result<Vec<Test>> {
    let mut tests: Vec<Test> = Vec::new();

    read_sequence_start(iter)?;
    while let Some(Ok(event)) = iter.next() {
        if event == Event::SequenceEnd {
            break;
        };
        let Event::SequenceStart { .. } = event else {
	    bail!("Expected SequenceStart, got {:?}", event)
	};
        tests.push(parse_test(iter)?);
    }
    Ok(tests)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let reader = File::open(args.yaml)?;
    let parser = libyaml::Parser::new(reader)?;
    let mut iter = parser.into_iter();

    read_stream_start(&mut iter)?;
    read_document_start(&mut iter)?;
    read_mapping_start(&mut iter)?;

    let mut test_suites: Vec<TestSuite> = Vec::new();
    let mut display_table = Default::default();
    let mut table: Table = Default::default();
    let mut test_mode: TestMode = TestMode::Forward;
    let mut tests: Vec<Test> = Vec::new();

    while let Some(Ok(event)) = iter.next() {
        match event {
            Event::Scalar { value, .. } => match value.as_str() {
                "display" => display_table = read_scalar(&mut iter)?.into(),
                "table" => table = parse_table(&mut iter)?,
                "flags" => test_mode = parse_flags(&mut iter)?,
                "tests" => tests = parse_tests(&mut iter)?,
                _ => bail!(""),
            },
            Event::MappingEnd => {
                break;
            }
            _ => {
                bail!("expected Scalar, got {:?}", event);
            }
        }
    }

    read_document_end(&mut iter)?;
    read_stream_end(&mut iter)?;

    let test_suite = TestSuite {
        display_table,
        table,
        mode: test_mode,
        tests,
    };

    test_suites.push(test_suite);

    let yaml = serde_yaml::to_string(&test_suites)?;

    match args.output {
        Some(path) => {
            fs::write(path, yaml)?;
        }
        None => {
            println!("{}", yaml);
        }
    }

    Ok(())
}
