use std::io::{self, Write};

use celestite_core::checked_byte_length;
use serde::Serialize;

use crate::DocumentError;

struct JsonBuffer {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl Write for JsonBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if checked_byte_length(self.bytes.len(), bytes.len(), self.limit).is_none() {
            self.overflowed = true;
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "JSON byte budget exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, DocumentError> {
    let mut buffer = JsonBuffer {
        bytes: Vec::new(),
        limit,
        overflowed: false,
    };
    let result = serde_json::to_writer(&mut buffer, value);
    if buffer.overflowed {
        return Err(DocumentError::TooLarge);
    }
    result?;
    Ok(buffer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_json_budget_round_trips_and_one_byte_less_is_refused()
    -> Result<(), Box<dyn std::error::Error>> {
        let text = "\0\n\r\"\\日本語🦀";
        let reference = serde_json::to_vec(text)?;
        assert_eq!(encode(&text, reference.len())?, reference);
        assert!(matches!(
            encode(&text, reference.len() - 1),
            Err(DocumentError::TooLarge)
        ));
        Ok(())
    }

    #[test]
    fn refused_append_preserves_the_prior_buffer() -> io::Result<()> {
        let mut buffer = JsonBuffer {
            bytes: vec![],
            limit: 2,
            overflowed: false,
        };
        buffer.write_all(b"ab")?;
        assert!(buffer.write_all(b"c").is_err());
        assert_eq!(buffer.bytes, b"ab");
        assert!(buffer.overflowed);
        buffer.flush()?;
        Ok(())
    }
}
