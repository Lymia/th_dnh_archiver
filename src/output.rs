use error::*;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fmt::{Display, Write as FmtWrite};
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
    if new_name.is_empty() {
        new_name.push_str("no_name")
    }

    new_name
}

fn normalize_path(path: &str) -> Vec<String> {
    let mut components = Vec::new();
    for component in path.replace('\\', "/").split('/') {
        if !component.trim().is_empty() {
            components.push(component.to_string());
        }
    }
    components
}

struct OutputDirNode {
    base_path: PathBuf, subdirs: HashMap<String, OutputDirNode>,
    encountered_names: HashSet<String>, written_names: HashSet<String>,
}
impl OutputDirNode {
    fn new(base_path: PathBuf) -> OutputDirNode {
        OutputDirNode {
            base_path, subdirs: HashMap::new(),
            encountered_names: HashSet::new(), written_names: HashSet::new(),
        }
    }

    fn uniq_name(
        &mut self, original_path: &str, path_name: &str, original_name: &str,
    ) -> Result<String> {
        if !self.base_path.exists() {
            create_dir_all(&self.base_path)?;
        }

        if self.encountered_names.contains(original_name) {
            eprintln!("WARNING: Duplicate file '{}' in archive.", original_path);
        }
        self.encountered_names.insert(original_name.to_string());

        let mut name = validate_filename(original_name);
        if name != original_name {
            eprintln!("WARNING: Invalid file name '{}' in archive.", original_path);
        }
        if self.written_names.contains(&name) {
            let mut suffix_count = 2;
            let mut final_name;
            while {
                final_name = format!("{}_{}", name, suffix_count);
                suffix_count += 1;
                self.written_names.contains(&final_name)
            } { }
            name = final_name;
        }
        if name != original_name {
            eprintln!("WARNING: Outputting '{}' as '{}{}'.", original_path, path_name, name);
        }
        self.written_names.insert(name.clone());

        Ok(name)
    }

    fn write_dir(
        &mut self, original_path: &str, path_name: &mut String, original_name: &str,
    ) -> Result<&mut OutputDirNode> {
        if let Some(node) = self.subdirs.get_mut(original_name) {
            Ok(node)
        } else {
            let mut path = self.base_path.clone();
            path.push(self.uniq_name(original_path, path_name.as_ref(), original_name)?);
            create_dir_all(&path)?;

            self.subdirs.insert(original_name.to_string(), OutputDirNode::new(path));
            Ok(self.subdirs.get_mut(original_name).unwrap())
        }
    }

    fn write_file(
        &mut self, original_path: &str, path_name: &str, original_name: &str,
    ) -> Result<impl Write> {
        let mut path = self.base_path.clone();
        path.push(self.uniq_name(original_path, path_name, original_name)?);
        Ok(BufWriter::new(File::create(path)?))
    }
}

pub struct Output {
    root: OutputDirNode, written_files: usize,
}
impl Output {
    pub fn for_path(path: impl AsRef<Path>) -> Result<Output> {
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
                create_dir_all(&base_path)?;
                return Ok(Output {
                    root: OutputDirNode::new(base_path), written_files: 0,
                })
            }

            file_name.clear();
            suffix_count += 1;
        }
    }

    pub fn display_out_path<'a>(&'a self) -> impl Display + 'a {
        self.root.base_path.display()
    }
    pub fn write_count(&self) -> usize {
        self.written_files
    }

    pub fn create(&mut self, dir: &str, name: &str) -> Result<impl Write> {
        let split = normalize_path(dir);

        let mut original_path = String::new();
        for component in &split {
            write!(original_path, "{}/", component)?;
        }
        original_path.push_str(name);

        let mut node = &mut self.root;
        let mut path_name = String::new();
        for component in &split {
            node = node.write_dir(&original_path, &mut path_name, component)?;
        }
        let out = node.write_file(&original_path, &path_name, name)?;
        self.written_files += 1;
        Ok(out)
    }
}