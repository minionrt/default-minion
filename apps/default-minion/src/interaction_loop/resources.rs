use std::collections::HashSet;

#[derive(Default)]
pub struct Resources {
    pub open_files: HashSet<String>,
}

impl Resources {
    pub fn add_file(&mut self, filename: &str) {
        self.open_files.insert(filename.to_owned());
    }
}
