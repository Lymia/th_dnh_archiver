use crate::{error::*, output::*};
use byteorder::*;
use encoding::{codec::japanese::Windows31JEncoding, DecoderTrap, Encoding};
use libflate::zlib::Decoder as ZlibDecoder;
use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom, Write},
    str::from_utf8,
};

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug)]
pub enum ArchiveType {
    Archive_012M,
    Archive_Ph3,
    NotAnArchive,
}

const ARCHIVE_012M_MAGIC: &[u8] = b"PACK_FILE\0";
const ARCHIVE_PH3_MAGIC: &[u8] = b"ArchiveFile";
const COMPRESS_ZIP_MAGIC: &[u8] = b"COMPRESS_ZIP\0";

fn check_archive_header_no_seek(mut read: impl Read, header: &[u8]) -> Result<bool> {
    let mut vec = Vec::with_capacity(header.len());
    unsafe { vec.set_len(header.len()) }
    read.read_exact(&mut vec)?;
    Ok(vec == header)
}
fn check_archive_header(mut read: impl Read + Seek, header: &[u8]) -> Result<bool> {
    read.seek(SeekFrom::Start(0))?;
    check_archive_header_no_seek(read, header)
}
pub fn determine_archive_type(mut read: impl Read + Seek) -> ArchiveType {
    if check_archive_header(&mut read, ARCHIVE_012M_MAGIC).unwrap_or(false) {
        ArchiveType::Archive_012M
    } else if check_archive_header(&mut read, ARCHIVE_PH3_MAGIC).unwrap_or(false) {
        ArchiveType::Archive_Ph3
    } else {
        ArchiveType::NotAnArchive
    }
}

fn transfer(out: &mut Output, dir: &str, name: &str, mut read: impl Read, size: u64) -> Result<()> {
    let mut write = out.create(dir, name)?;

    let mut buffer = [0u8; 1024 * 64];
    let mut remaining = size;
    while remaining > 0 {
        let read_bytes = read.read(&mut buffer)?;
        if read_bytes == 0 {
            if remaining > 0 {
                eprintln!(
                    "WARNING: Entry '{}' ended prematurely. (expected {} bytes, got {})",
                    name,
                    size,
                    size - remaining
                );
            }
            break;
        } else if read_bytes as u64 > remaining {
            write.write_all(&buffer[..remaining as usize])?;
            remaining = 0;
        } else {
            write.write_all(&buffer[..read_bytes])?;
            remaining -= read_bytes as u64;
        }
    }

    let is_eof = read.read(&mut buffer[0..1])? == 0;
    if !is_eof {
        eprintln!(
            "WARNING: Entry '{}' contains more data than header suggests. \
                   Truncating at {} bytes.",
            name, size
        );
    }

    Ok(())
}

fn read_cstr(mut read: impl Read) -> Result<String> {
    let size = read.read_u32::<LE>()? as usize;
    let mut vec = Vec::new();
    let mut zero_encountered = false;
    for _ in 0..size {
        let byte = read.read_u8()?;
        if byte == 0 || zero_encountered {
            zero_encountered = true
        } else {
            vec.push(byte)
        }
    }

    match from_utf8(&vec) {
        Ok(rstr) => Ok(rstr.to_string()),
        Err(_) => match Windows31JEncoding.decode(&vec, DecoderTrap::Strict) {
            Ok(rstr) => Ok(rstr),
            Err(_) => {
                let rstr = String::from_utf8_lossy(&vec);
                eprintln!("WARNING: Entry '{}' has an invalid UTF-8 or SJIS name.", rstr);
                Ok(rstr.to_string())
            }
        },
    }
}
struct FileEntry {
    name: String,
    offset: u64,
    len: u64,
}
pub fn extract_012m(file: File, out: &mut Output) -> Result<()> {
    let mut file = BufReader::new(file);

    assert!(check_archive_header(&mut file, ARCHIVE_012M_MAGIC)?);
    let file_count = file.read_u32::<LE>()?;

    let mut entries = Vec::new();
    for _ in 0..file_count {
        let name = read_cstr(&mut file)?;
        let offset = file.read_u32::<LE>()? as u64;
        let len = file.read_u32::<LE>()? as u64;
        entries.push(FileEntry { name, offset, len })
    }
    for FileEntry { name, offset, len } in entries {
        file.seek(SeekFrom::Start(offset))?;
        if check_archive_header_no_seek(&mut file, COMPRESS_ZIP_MAGIC).unwrap_or(false) {
            let uncompressed_len = file.read_u32::<LE>()? as u64;
            let compressed_len = len - 4 - COMPRESS_ZIP_MAGIC.len() as u64;
            let in_stream = (&mut file).take(compressed_len);
            transfer(out, "", &name, ZlibDecoder::new(in_stream)?, uncompressed_len)?;
        } else {
            file.seek(SeekFrom::Start(offset))?;
            let in_stream = (&mut file).take(len);
            transfer(out, "", &name, in_stream, len)?;
        }
    }

    Ok(())
}

fn read_wchar_str(mut read: impl Read) -> Result<String> {
    let count = read.read_u32::<LE>()? as usize;
    let mut vec = Vec::new();
    for _ in 0..count {
        vec.push(read.read_u16::<LE>()?);
    }
    match String::from_utf16(&vec) {
        Ok(rstr) => Ok(rstr),
        Err(_) => {
            let rstr = String::from_utf16_lossy(&vec);
            eprintln!("WARNING: Entry '{}' has an invalid UTF-16 name.", rstr);
            Ok(rstr)
        }
    }
}
fn extract_ph3_inner(
    mut header_stream: impl Read,
    mut contents_stream: impl Read + Seek,
    file_count: u32,
    out: &mut Output,
) -> Result<()> {
    for _ in 0..file_count {
        let _entry_len = header_stream.read_u32::<LE>()? as u64;
        let dir_name = read_wchar_str(&mut header_stream)?;
        let entry_name = read_wchar_str(&mut header_stream)?;
        let is_compressed = header_stream.read_u32::<LE>()? != 0;
        let uncompressed_len = header_stream.read_u32::<LE>()? as u64;
        let compressed_len = header_stream.read_u32::<LE>()? as u64;
        let offset = header_stream.read_u32::<LE>()? as u64;

        contents_stream.seek(SeekFrom::Start(offset))?;
        if is_compressed {
            let in_stream = (&mut contents_stream).take(compressed_len);
            transfer(out, &dir_name, &entry_name, ZlibDecoder::new(in_stream)?, uncompressed_len)?;
        } else {
            let in_stream = (&mut contents_stream).take(uncompressed_len);
            transfer(out, &dir_name, &entry_name, in_stream, uncompressed_len)?;
        }
    }
    Ok(())
}
pub fn extract_ph3(file: File, out: &mut Output) -> Result<()> {
    let mut file = BufReader::new(file);

    assert!(check_archive_header(&mut file, ARCHIVE_PH3_MAGIC)?);
    let file_count = file.read_u32::<LE>()?;
    let is_compressed = file.read_u8()? != 0;
    let header_size = file.read_u32::<LE>()? as u64;
    assert!(header_size <= usize::max_value() as u64);
    let mut header = vec![0u8; header_size as usize];
    file.read_exact(&mut header)?;

    if is_compressed {
        extract_ph3_inner(ZlibDecoder::new(Cursor::new(header))?, file, file_count, out)
    } else {
        extract_ph3_inner(Cursor::new(header), file, file_count, out)
    }
}
