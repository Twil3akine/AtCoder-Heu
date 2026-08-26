use std::fmt::Display;
use std::io::{self, BufWriter, Read, Write};
use std::str::FromStr;

/// Whitespace-token input reader for contest programs.
pub struct Input {
    data: Vec<u8>,
    index: usize,
}

impl Input {
    pub fn new() -> Self {
        let mut data = Vec::new();
        io::stdin()
            .read_to_end(&mut data)
            .expect("failed to read standard input");
        Self { data, index: 0 }
    }

    pub fn has_next(&mut self) -> bool {
        self.skip_whitespace();
        self.index
            < self
                .data
                .len()
    }

    pub fn read<T: FromStr>(&mut self) -> T
    where
        T::Err: std::fmt::Debug,
    {
        let token = self.token();
        std::str::from_utf8(token)
            .expect("input token is not valid UTF-8")
            .parse()
            .expect("failed to parse input token")
    }

    pub fn vec<T: FromStr>(
        &mut self,
        length: usize,
    ) -> Vec<T>
    where
        T::Err: std::fmt::Debug,
    {
        (0..length)
            .map(|_| self.read())
            .collect()
    }

    pub fn matrix<T: FromStr>(
        &mut self,
        rows: usize,
        columns: usize,
    ) -> Vec<Vec<T>>
    where
        T::Err: std::fmt::Debug,
    {
        (0..rows)
            .map(|_| self.vec(columns))
            .collect()
    }

    pub fn string(&mut self) -> String {
        String::from_utf8(
            self.token()
                .to_vec(),
        )
        .expect("input token is not valid UTF-8")
    }

    pub fn chars(&mut self) -> Vec<char> {
        self.string()
            .chars()
            .collect()
    }

    pub fn bytes(&mut self) -> Vec<u8> {
        self.token()
            .to_vec()
    }

    pub fn usize1(&mut self) -> usize {
        self.read::<usize>()
            .checked_sub(1)
            .expect("usize1 requires a positive integer")
    }

    fn skip_whitespace(&mut self) {
        while self.index
            < self
                .data
                .len()
            && self.data[self.index].is_ascii_whitespace()
        {
            self.index += 1;
        }
    }

    fn token(&mut self) -> &[u8] {
        self.skip_whitespace();
        assert!(
            self.index
                < self
                    .data
                    .len(),
            "unexpected end of input"
        );
        let start = self.index;
        while self.index
            < self
                .data
                .len()
            && !self.data[self.index].is_ascii_whitespace()
        {
            self.index += 1;
        }
        &self.data[start..self.index]
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

/// Buffered standard-output writer. It flushes automatically on drop.
pub struct Output {
    buffer: OutputBuffer<BufWriter<io::Stdout>>,
}

impl Output {
    pub fn new() -> Self {
        Self {
            buffer: OutputBuffer::new(BufWriter::new(io::stdout())),
        }
    }

    pub fn print<T: Display>(
        &mut self,
        value: T,
    ) {
        self.buffer
            .print(value)
            .expect("failed to write output");
    }

    pub fn println<T: Display>(
        &mut self,
        value: T,
    ) {
        self.buffer
            .println(value)
            .expect("failed to write output");
    }

    pub fn join<I, T>(
        &mut self,
        values: I,
        separator: &str,
    ) where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        self.buffer
            .join(values, separator)
            .expect("failed to write output");
    }

    pub fn join_space<I, T>(
        &mut self,
        values: I,
    ) where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        self.buffer
            .join_space(values)
            .expect("failed to write output");
    }

    pub fn join_line<I, T>(
        &mut self,
        values: I,
    ) where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        self.buffer
            .join_line(values)
            .expect("failed to write output");
    }

    pub fn yes_no(
        &mut self,
        yes: bool,
    ) {
        self.buffer
            .yes_no(yes)
            .expect("failed to write output");
    }

    pub fn flush(&mut self) {
        flush_or_panic(&mut self.buffer);
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // Never turn an existing panic into a double panic during unwinding.
        flush_ignoring_error(&mut self.buffer);
    }
}

struct OutputBuffer<W: Write> {
    writer: W,
}

impl<W: Write> OutputBuffer<W> {
    fn new(writer: W) -> Self {
        Self { writer }
    }

    fn print<T: Display>(
        &mut self,
        value: T,
    ) -> io::Result<()> {
        write!(self.writer, "{value}")
    }

    fn println<T: Display>(
        &mut self,
        value: T,
    ) -> io::Result<()> {
        writeln!(self.writer, "{value}")
    }

    fn join<I, T>(
        &mut self,
        values: I,
        separator: &str,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        let mut first = true;
        for value in values {
            if !first {
                write!(self.writer, "{separator}")?;
            }
            first = false;
            write!(self.writer, "{value}")?;
        }
        writeln!(self.writer)
    }

    fn join_space<I, T>(
        &mut self,
        values: I,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        self.join(values, " ")
    }

    fn join_line<I, T>(
        &mut self,
        values: I,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        self.join(values, "\n")
    }

    fn yes_no(
        &mut self,
        yes: bool,
    ) -> io::Result<()> {
        self.println(if yes { "Yes" } else { "No" })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer
            .flush()
    }

    #[cfg(test)]
    fn into_inner(self) -> W {
        self.writer
    }
}

fn flush_or_panic<W: Write>(buffer: &mut OutputBuffer<W>) {
    buffer
        .flush()
        .expect("failed to flush output");
}

fn flush_ignoring_error<W: Write>(buffer: &mut OutputBuffer<W>) {
    let _ = buffer.flush();
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::{flush_ignoring_error, flush_or_panic, Input, OutputBuffer};

    fn input(text: &str) -> Input {
        Input {
            data: text
                .as_bytes()
                .to_vec(),
            index: 0,
        }
    }

    #[test]
    fn reads_common_token_shapes() {
        let mut input = input("3 4 5 6 abc xyz 2");
        assert!(input.has_next());
        assert_eq!(input.read::<usize>(), 3);
        assert_eq!(input.vec::<i32>(2), [4, 5]);
        assert_eq!(input.matrix::<u8>(1, 1), [vec![6]]);
        assert_eq!(input.string(), "abc");
        assert_eq!(input.chars(), ['x', 'y', 'z']);
        assert_eq!(input.usize1(), 1);
        assert!(!input.has_next());
    }

    #[test]
    fn skips_newlines_multiple_spaces_and_reads_negative_values() {
        let mut input = input("\n  -7\n\n  12   \t-3  \n");
        assert_eq!(input.read::<i64>(), -7);
        assert_eq!(input.read::<i64>(), 12);
        assert_eq!(input.read::<i64>(), -3);
        assert!(!input.has_next());
    }

    #[test]
    fn bytes_use_the_raw_token() {
        let mut input = input("\u{00e9}");
        assert_eq!(input.bytes(), "\u{00e9}".as_bytes());
    }

    #[test]
    #[should_panic(expected = "unexpected end of input")]
    fn read_panics_at_eof() {
        let mut input = input("");
        let _: usize = input.read();
    }

    #[test]
    #[should_panic(expected = "failed to parse input token")]
    fn read_panics_on_parse_error() {
        let mut input = input("x");
        let _: usize = input.read();
    }

    #[test]
    #[should_panic(expected = "usize1 requires a positive integer")]
    fn usize1_panics_for_zero() {
        let mut input = input("0");
        input.usize1();
    }

    #[test]
    fn generic_output_buffer_writes_all_public_shapes() {
        let mut output = OutputBuffer::new(Vec::new());
        output
            .print("a")
            .unwrap();
        output
            .println(12)
            .unwrap();
        output
            .join([3, 4, 5], ",")
            .unwrap();
        output
            .join_space([6, 7])
            .unwrap();
        output
            .join_line([8, 9])
            .unwrap();
        output
            .yes_no(true)
            .unwrap();
        output
            .yes_no(false)
            .unwrap();
        output
            .join(Vec::<i32>::new(), " ")
            .unwrap();
        output
            .flush()
            .unwrap();
        assert_eq!(
            String::from_utf8(output.into_inner()).unwrap(),
            "a12\n3,4,5\n6 7\n8\n9\nYes\nNo\n\n"
        );
    }

    struct FlushError;

    impl Write for FlushError {
        fn write(
            &mut self,
            _buffer: &[u8],
        ) -> io::Result<usize> {
            Ok(0)
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush error"))
        }
    }

    #[test]
    #[should_panic(expected = "failed to flush output")]
    fn explicit_flush_panics_on_error() {
        let mut output = OutputBuffer::new(FlushError);
        flush_or_panic(&mut output);
    }

    #[test]
    fn drop_flush_path_ignores_error() {
        let mut output = OutputBuffer::new(FlushError);
        flush_ignoring_error(&mut output);
    }
}
