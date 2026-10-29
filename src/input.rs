use crate::error::Error;
use flate2::read::MultiGzDecoder;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct Source {
    pub name: String,
    pub reader: Box<dyn BufRead>,
}

pub fn open_sources(paths: &[PathBuf]) -> Result<Vec<Source>, Error> {
    if paths.is_empty() {
        return Ok(vec![Source {
            name: "<stdin>".to_string(),
            reader: Box::new(BufReader::new(io::stdin())),
        }]);
    }

    let mut sources = Vec::with_capacity(paths.len());

    for path in paths {
        if path == Path::new("-") {
            sources.push(Source {
                name: "<stdin>".to_string(),
                reader: Box::new(BufReader::new(io::stdin())),
            });
        } else {
            let reader = open_file(path)?;
            sources.push(Source {
                name: path.display().to_string(),
                reader,
            });
        }
    }

    Ok(sources)
}

fn open_file(path: &Path) -> Result<Box<dyn BufRead>, Error> {
    let name = path.display().to_string();
    let mut file = File::open(path).map_err(|err| Error::io(Some(&name), err))?;

    let mut magic = [0u8; 2];
    let read = file
        .read(&mut magic)
        .map_err(|err| Error::io(Some(&name), err))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|err| Error::io(Some(&name), err))?;

    if read == 2 && magic == [0x1f, 0x8b] {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

pub fn detect_delimiter(reader: &mut dyn BufRead, requested: &str) -> Result<u8, Error> {
    match requested {
        "lf" | "crlf" => Ok(b'\n'),
        "cr" => Ok(b'\r'),
        "nul" => Ok(0),
        "auto" => {
            let buffer = reader.fill_buf()?;
            let sample = &buffer[..buffer.len().min(65_536)];
            if sample.contains(&0) {
                Ok(0)
            } else {
                Ok(b'\n')
            }
        }
        other => Err(Error::usage(format!(
            "invalid delimiter '{other}' (expected auto, lf, crlf, cr, or nul)"
        ))),
    }
}
