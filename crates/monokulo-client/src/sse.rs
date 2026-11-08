//! A small reader of server-sent events (the WHATWG `text/event-stream`
//! format), fed the body's bytes as they arrive. It knows only what
//! Monokulo's stream uses: `event` and `data` fields, and comments, which it
//! skips. `id` and `retry` are read and ignored, since the caller, not the
//! reader, reconnects.

/// One event: its name (`None` when it had no `event` field) and its data,
/// lines joined with `\n`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Event {
    pub name: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub(crate) struct Reader {
    /// Bytes of a line not yet ended.
    line: Vec<u8>,
    /// The last byte was a `\r`, so a `\n` next ends nothing more.
    after_cr: bool,
    name: Option<String>,
    data: Option<String>,
}

impl Reader {
    /// Reads `bytes` on from where the last call stopped, and returns the
    /// events they completed.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        for &byte in bytes {
            match byte {
                b'\n' if self.after_cr => self.after_cr = false,
                b'\r' | b'\n' => {
                    self.after_cr = byte == b'\r';
                    let line = std::mem::take(&mut self.line);
                    if let Some(event) = self.line_ended(&String::from_utf8_lossy(&line)) {
                        events.push(event);
                    }
                }
                _ => {
                    self.after_cr = false;
                    self.line.push(byte);
                }
            }
        }
        events
    }

    fn line_ended(&mut self, line: &str) -> Option<Event> {
        if line.is_empty() {
            // An event ends; one with no data isn't dispatched.
            let name = self.name.take();
            return self.data.take().map(|data| Event { name, data });
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.name = Some(value.to_owned()),
            "data" => match &mut self.data {
                Some(data) => {
                    data.push('\n');
                    data.push_str(value);
                }
                None => self.data = Some(value.to_owned()),
            },
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: Option<&str>, data: &str) -> Event {
        Event {
            name: name.map(Into::into),
            data: data.into(),
        }
    }

    #[test]
    fn events_are_read_across_chunks_skipping_comments() {
        let mut reader = Reader::default();
        let mut events = reader.push(b"event: ready\ndata: {\"at\"");
        assert!(events.is_empty());
        events.extend(reader.push(b":1}\n\n: keep-alive\n\ndata: a\r"));
        events.extend(reader.push(b"\ndata:b\r\rid: 7\nevent: x\n\n"));
        assert_eq!(
            events,
            [event(Some("ready"), r#"{"at":1}"#), event(None, "a\nb")]
        );
        assert!(
            reader.push(b"\n").is_empty(),
            "an event with no data is dropped, its name with it"
        );
        assert_eq!(reader.push(b"data\n\n"), [event(None, "")]);
    }

    #[test]
    fn a_character_split_between_chunks_survives() {
        let mut reader = Reader::default();
        let bytes = "data: €\n\n".as_bytes();
        let mut events = reader.push(&bytes[..7]);
        events.extend(reader.push(&bytes[7..]));
        assert_eq!(events, [event(None, "€")]);
    }
}
