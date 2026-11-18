use crate::error::Error;
use flate2::read::MultiGzDecoder;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// A follow source that tails a file for new lines.
///
/// On construction it seeks to the end of the file. Each call to
/// `read_new_lines` returns any lines appended since the last call.
pub struct FollowSource {
    name: String,
    file: File,
    offset: u64,
    delimiter: u8,
    buffer: Vec<u8>,
}

impl FollowSource {
    pub fn open(path: &Path, delimiter: u8) -> Result<Self, Error> {
        let name = path.display().to_string();
        let mut file = File::open(path).map_err(|err| Error::io(Some(&name), err))?;
        let offset = file
            .seek(SeekFrom::End(0))
            .map_err(|err| Error::io(Some(&name), err))?;
        Ok(Self {
            name,
            file,
            offset,
            delimiter,
            buffer: Vec::new(),
        })
    }

    /// Read any newly appended lines since the last call.
    pub fn read_new_lines(&mut self) -> io::Result<Vec<String>> {
        let mut lines = Vec::new();

        // Seek to the last known offset.
        self.file.seek(SeekFrom::Start(self.offset))?;

        self.buffer.clear();
        let mut read_any = false;

        let mut byte_buf = [0u8; 1];

        loop {
            let n = match self.file.read(&mut byte_buf)? {
                0 => break,
                n => n,
            };
            if n == 0 {
                break;
            }
            let byte = byte_buf[0];
            read_any = true;
            self.buffer.push(byte);

            if byte == self.delimiter {
                if self.buffer.last() == Some(&self.delimiter) {
                    self.buffer.pop();
                }
                if self.delimiter != b'\r' && self.buffer.last() == Some(&b'\r') {
                    self.buffer.pop();
                }
                if !self.buffer.is_empty() {
                    let line = String::from_utf8_lossy(&self.buffer).to_string();
                    lines.push(line);
                }
                self.buffer.clear();
            }
        }

        if read_any {
            self.offset = self.file.stream_position()?;
        }

        // If there's leftover buffer without a delimiter, keep it for next time
        // (don't advance offset past incomplete line).
        if !self.buffer.is_empty() {
            // We consumed bytes but didn't emit a complete line; rewind offset
            // so we re-read from the start of the incomplete line next time.
            let incomplete_len = self.buffer.len() as u64;
            let current = self.file.stream_position()?;
            self.offset = current.saturating_sub(incomplete_len);
            self.file.seek(SeekFrom::Start(self.offset))?;
            self.buffer.clear();
        }

        Ok(lines)
    }

    /// Poll for new lines with a timeout, returning when data is available or
    /// the timeout elapses.
    pub fn poll_new_lines(&mut self, timeout: Duration) -> io::Result<Option<Vec<String>>> {
        let start = Instant::now();
        loop {
            let lines = self.read_new_lines()?;
            if !lines.is_empty() {
                return Ok(Some(lines));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            // Small sleep to avoid busy-waiting.
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
