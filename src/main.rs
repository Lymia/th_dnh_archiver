#![windows_subsystem = "console"]

extern crate byteorder;
extern crate encoding;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate libflate;

mod error {
    pub use failure::Error;
    pub type Result<T> = ::std::result::Result<T, Error>;
}

mod archive;
mod output;

// Main method
//////////////

use std::{
    env,
    ffi::OsString,
    fs::File,
    io::stdin,
    mem,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    process,
    time::Instant,
};

fn extract(path: PathBuf) -> error::Result<()> {
    let mut file = File::open(&path)?;
    let arc_type = archive::determine_archive_type(&mut file);
    if let archive::ArchiveType::NotAnArchive = arc_type {
        eprintln!("File '{}' is not a Danmakufu 0.12m or ph3 archive.", path.display());
        return Ok(());
    }

    let mut output = output::Output::for_path(&path)?;
    println!("Extracting '{}' to '{}'...", path.display(), output.display_out_path());
    let start_time = Instant::now();

    match arc_type {
        archive::ArchiveType::Archive_Ph3 => archive::extract_ph3(file, &mut output)?,
        archive::ArchiveType::Archive_012M => archive::extract_012m(file, &mut output)?,
        _ => unreachable!(),
    }

    let total_time = Instant::now().duration_since(start_time);
    println!(
        "Extracted {} files in {} ms.",
        output.write_count(),
        total_time.as_secs() * 1000 + total_time.subsec_millis() as u64
    );
    Ok(())
}

fn press_any_key() {
    eprint!("Press Enter to continue... ");
    stdin().read_line(&mut String::new()).unwrap();
}
fn after_error() {
    press_any_key();
    process::exit(1);
}

fn main() {
    env::set_var("RUST_BACKTRACE", "1");

    let mut args: Vec<OsString> = env::args_os().collect();
    if args.len() != 2 {
        eprintln!("To extract a .dat file, drag it onto {}.", args[0].to_string_lossy());
        eprintln!(
            "To create a .dat file, drag a single directory onto {}.",
            args[0].to_string_lossy()
        );
        eprintln!(
            "Alternatively, if you are using a terminal, please use: \
                   {} [archive to extract]",
            args[0].to_string_lossy()
        );
        after_error();
    }
    let target_file = args.pop().unwrap();
    mem::drop(args);
    let path = PathBuf::from(target_file);
    if !path.exists() {
        eprintln!("No such file '{}' exists.", path.display());
        after_error();
    }
    if !path.is_file() {
        eprintln!("'{}' is not a regular file.", path.display());
        after_error();
    }
    match catch_unwind(AssertUnwindSafe(|| extract(path))) {
        Ok(Ok(())) | Err(_) => press_any_key(),
        Ok(Err(err)) => {
            eprintln!("Error: {}\n{}", err, err.backtrace());
            after_error();
        }
    }
}
