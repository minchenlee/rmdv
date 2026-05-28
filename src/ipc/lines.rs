pub struct ByteToLine {
    /// Byte offset of the start of each line (1-indexed line number = index + 1).
    line_starts: Vec<u32>,
}

impl ByteToLine {
    /// 1-indexed line number for a byte offset. Returns the last line if `byte`
    /// is past the end. Empty source returns 1.
    pub fn line_for_byte(&self, byte: usize) -> u32 {
        if self.line_starts.is_empty() {
            return 1;
        }
        let byte = byte as u32;
        match self.line_starts.binary_search(&byte) {
            Ok(i) => (i as u32) + 1,
            Err(0) => 1,
            Err(i) => i as u32,
        }
    }
}

pub fn build_byte_to_line(src: &str) -> ByteToLine {
    let mut starts = vec![0u32];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    ByteToLine {
        line_starts: starts,
    }
}

/// Largest index `i` such that `block_lines[i] <= line`. Returns `Some(0)` if
/// `line` precedes the first block (clamp). Returns `None` only if the slice is
/// empty.
pub fn block_for_line(line: u32, block_lines: &[u32]) -> Option<usize> {
    if block_lines.is_empty() {
        return None;
    }
    match block_lines.binary_search(&line) {
        Ok(i) => Some(i),
        Err(0) => Some(0),
        Err(i) => Some(i - 1),
    }
}
