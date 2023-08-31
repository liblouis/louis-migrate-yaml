use serde::Serialize;

use std::{fs::{File, self}, path::PathBuf};

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
    output: Option<PathBuf>
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

fn is_false(b: &bool) -> bool {
    *b == false
}

#[derive(Debug, Default, Serialize)]
pub struct Test {
    input: String,
    expected: String,
    #[serde(skip_serializing_if = "is_false")]
    xfail: bool
}

fn parse_display_table(iter: &mut ParserIter) -> Result<PathBuf> {
    match iter.next() {
	Some(Ok(Event::Scalar { value, .. })) => Ok(value.into()),
	_ => bail!("expected Scalar for display")
    }
}

fn parse_table(iter: &mut ParserIter) -> Result<Table> {
    let Some(Ok(Event::MappingStart { .. })) = iter.next() else {
	bail!("expected MappingStart")
    };
    let mut table: Table = Default::default();
    while let Some(Ok(event)) = iter.next() {
	match event {
            Event::Scalar { ref value, .. } if value == "language" => {
		if let Some(Ok(Event::Scalar { value, .. })) = iter.next() {
		    table = Table{language: value.into(), ..table}
		} else {
		    bail!("Epected Scalar");
		}
	    },
            Event::Scalar { ref value, .. } if value == "grade" => {
		if let Some(Ok(Event::Scalar { value, .. })) = iter.next() {
		    table = Table{grade: value.parse::<u8>()?, ..table}
		} else {
		    bail!("Epected Scalar");
		}
	    },
            Event::Scalar { ref value, .. } if value == "system" => {
		if let Some(Ok(Event::Scalar { value, .. })) = iter.next() {
		    table = Table{system: value.into(), ..table};
		} else {
		    bail!("Epected Scalar");
		}
	    },
            Event::Scalar { ref value, .. } if value == "__assert-match" => {
		if let Some(Ok(Event::Scalar { value, .. })) = iter.next() {
		    table = Table{path: value.into(), ..table};
		} else {
		    bail!("Epected Scalar");
		}
	    },
	    Event::MappingEnd => {break;},
	    _ => bail!("Event {:?}", event)
	};
    }
    Ok(table)
}

fn parse_flags(iter: &mut ParserIter) -> Result<TestMode> {
    let Some(Ok(Event::MappingStart { .. })) = iter.next() else {
	bail!("expected MappingStart")
    };
    if let Some(Ok(event)) = iter.next() {
	let mode = match event {
            Event::Scalar { ref value, .. } if value == "forward" => TestMode::Forward,
            Event::Scalar { ref value, .. } if value == "backward" => TestMode::Backward,
            Event::Scalar { ref value, .. } if value == "bothDirections" => TestMode::BothDirections,
            Event::Scalar { ref value, .. } if value == "display" => TestMode::Display,
            Event::Scalar { ref value, .. } if value == "hyphenate" => TestMode::Hyphenate,
            Event::Scalar { ref value, .. } if value == "hyphenateBraille" => TestMode::HyphenateBraille,
	    _ => bail!("Testmode {:?} not supported", event)
	};
	let Some(Ok(Event::MappingEnd)) = iter.next() else {
	    bail!("expected MappingEnd")
	};
	Ok(mode)
    } else {
	bail!("Expected Scalar");
    }
}

fn parse_test(iter: &mut ParserIter) -> Result<Test> {
    let input: String;
    let expected: String;
    let xfail = false;
    if let Some(Ok(Event::Scalar { value, .. })) = iter.next() {
	input = value.into();
    } else {
	bail!("expected Scalar")
    };
    if let Some(Ok(Event::Scalar { value, .. })) = iter.next() {
	expected = value.into();
    } else {
	bail!("expected Scalar")
    };
    let Some(Ok(Event::SequenceEnd)) = iter.next() else {
	bail!("expected SequenceEnd")
    };
    Ok(Test{input, expected, xfail })
}

fn parse_tests(iter: &mut ParserIter) -> Result<Vec<Test>> {
    let mut tests: Vec<Test> = Vec::new();
    let Some(Ok(Event::SequenceStart { .. })) = iter.next() else {
	bail!("expected SequenceStart")
    };
    while let Some(Ok(event)) = iter.next() {
        if event == Event::SequenceEnd {
            break;
        };
	let Event::SequenceStart { .. } = event else {
	    bail!("expected SequenceStart, got {:?}", event)
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

    if let Some(Ok(Event::StreamStart {
        encoding: Some(encoding),
    })) = iter.next()
    {
        if encoding != Encoding::Utf8 {
            bail!("Encoding {:?} not supported", encoding);
        }
    } else {
        bail!("Expected event")
    };

    let Some(Ok(Event::DocumentStart { .. })) = iter.next() else {
	bail!("expected DocumentStart")
    };

    let Some(Ok(Event::MappingStart { .. })) = iter.next() else {
	bail!("expected MappingStart")
    };

    let mut display_table = Default::default();
    let mut table: Table = Default::default();
    let mut test_mode: TestMode = TestMode::Forward;
    let mut tests: Vec<Test> = Vec::new();
    
    while let Some(Ok(event)) = iter.next() {
        match event {
            Event::Scalar { ref value, .. } if value == "display" => display_table = parse_display_table(&mut iter)?,
            Event::Scalar { ref value, .. } if value == "table" => table = parse_table(&mut iter)?,
            Event::Scalar { ref value, .. } if value == "flags" => test_mode = parse_flags(&mut iter)?,
            Event::Scalar { ref value, .. } if value == "tests" => tests = parse_tests(&mut iter)?,
	    Event::MappingEnd => {break;}
            _ => {
                bail!("expected Scalar, got {:?}", event);
            }
        }
    }

    let Some(Ok(Event::DocumentEnd { .. })) = iter.next() else {
	bail!("expected DocumentEnd")
    };

    let Some(Ok(Event::StreamEnd)) = iter.next() else {
	bail!("expected StreamEnd")
    };

    let test_suite = TestSuite{ display_table, table, mode: test_mode, tests };
    let yaml = serde_yaml::to_string(&test_suite)?;

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
