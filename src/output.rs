use error::*;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Display;
use std::fs::{File, create_dir_all};
use std::io::{Write, BufWriter};
use std::path::{Path, PathBuf};

pub fn validate_filename(name: &str) -> String {
    let mut new_name = String::new();
    for char in name.trim().trim_right_matches('.').chars() {
        match char {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => new_name.push('_'),
            '\0' ... '\u{1F}' => new_name.push('_'),
            x => new_name.push(x),
        }
    }

    lazy_static! {
        static ref INVALID_NAMES: HashSet<&'static str> = {
            let mut set = HashSet::new();
            for &i in &[
                ".", "..",
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                "LPT8",
            ] {
                set.insert(i);
            }
            set
        };
    }
    if INVALID_NAMES.contains(name.to_ascii_uppercase().as_str()) {
        new_name.push('_')
    }

    new_name
}

pub struct OutputDir {
    base_path: PathBuf, encountered_files: HashSet<String>, written_files: HashSet<String>,
}
impl OutputDir {
    pub fn for_path(path: impl AsRef<Path>) -> Result<OutputDir> {
        let path = path.as_ref();

        let base_file_name = match path.file_stem() {
            Some(name) => name.to_owned(),
            None => bail!("Could not get file name for '{}'", path.display()),
        };

        let mut suffix_count = 1;
        let mut base_path = path.to_owned();
        let mut file_name = OsString::new();
        loop {
            file_name.push(&base_file_name);
            if suffix_count == 1 {
                file_name.push("_extracted");
            } else {
                file_name.push(format!("_extracted_{}", suffix_count));
            }
            base_path.set_file_name(&file_name);
            if !base_path.exists() {
                return Ok(OutputDir {
                    base_path, encountered_files: HashSet::new(), written_files: HashSet::new(),
                })
            }

            file_name.clear();
            suffix_count += 1;
        }
    }

    pub fn display_out_path<'a>(&'a self) -> impl Display + 'a {
        self.base_path.display()
    }

    pub fn write_count(&self) -> usize {
        self.written_files.len()
    }

    pub fn create(&mut self, original_name: &str) -> Result<impl Write> {
        if !self.base_path.exists() {
            create_dir_all(&self.base_path)?;
        }

        if self.encountered_files.contains(original_name) {
            eprintln!("WARNING: Duplicate file '{}' in archive.",
                      original_name);
        }
        self.encountered_files.insert(original_name.to_owned());

        let mut name = validate_filename(original_name);
        if name != original_name {
            eprintln!("WARNING: Invalid file name '{}' in archive. Outputting as '{}'.",
                      original_name, name);
        }

        if self.written_files.contains(&name) {
            let mut suffix_count = 2;
            let mut final_name;
            while {
                final_name = format!("{}_{}", name, suffix_count);
                suffix_count += 1;
                self.written_files.contains(&final_name)
            } { }
            eprintln!("WARNING: Outputting duplicate file '{}' as '{}'",
                      original_name, final_name);
            name = final_name;
        }

        self.written_files.insert(name.clone());

        let mut path = self.base_path.clone();
        path.push(name);
        Ok(BufWriter::new(File::create(path)?))
    }
}