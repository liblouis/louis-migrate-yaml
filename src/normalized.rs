use serde::Serialize;

use std::{collections::HashSet, path::PathBuf};

#[derive(Debug, Serialize, Hash)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Default, Debug, Serialize)]
pub struct TestSuite {
    display: PathBuf,
    // FIXME: instead of a reference to a file a test should rather
    // contain something that can be constructed in a test such as a
    // TranslationTable
    table: PathBuf,
    directions: HashSet<Direction>,
    tests: Vec<Test>,
}

#[derive(Default, Debug, Serialize)]
pub struct Test {
    input: String,
    expected: String,
    xfail: bool,
}
