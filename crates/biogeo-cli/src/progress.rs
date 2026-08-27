use std::io::{self, Write};

pub const CLI_PROGRESS_FORMAT: &str = "biogeo-cli-progress-v1";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProgressOutputFormat {
    #[default]
    None,
    Tsv,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProgressEvent<'a> {
    pub event: &'a str,
    pub command: &'a str,
    pub dataset_id: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub completed: Option<usize>,
    pub total: Option<usize>,
    pub start: Option<usize>,
    pub starts: Option<usize>,
    pub iteration: Option<usize>,
    pub max_iterations: Option<usize>,
    pub evaluations: Option<usize>,
    pub best_log_likelihood: Option<f64>,
}

#[derive(Debug)]
pub struct ProgressReporter {
    format: ProgressOutputFormat,
    sequence: u64,
}

impl ProgressReporter {
    pub fn new(format: ProgressOutputFormat) -> Self {
        Self {
            format,
            sequence: 0,
        }
    }

    #[cfg(test)]
    pub fn disabled() -> Self {
        Self::new(ProgressOutputFormat::None)
    }

    pub fn emit(&mut self, event: ProgressEvent<'_>) -> io::Result<()> {
        if self.format == ProgressOutputFormat::None {
            return Ok(());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("biogeo CLI progress sequence overflowed u64"))?;
        let line = format_event(self.sequence, event);
        let stderr = io::stderr();
        let mut writer = stderr.lock();
        writer.write_all(line.as_bytes())?;
        writer.flush()
    }
}

fn format_event(sequence: u64, event: ProgressEvent<'_>) -> String {
    let fields = vec![
        CLI_PROGRESS_FORMAT.to_string(),
        sequence.to_string(),
        encode_field(event.event),
        encode_field(event.command),
        encode_optional_field(event.dataset_id),
        encode_optional_field(event.model_id),
        encode_optional_usize(event.completed),
        encode_optional_usize(event.total),
        encode_optional_usize(event.start),
        encode_optional_usize(event.starts),
        encode_optional_usize(event.iteration),
        encode_optional_usize(event.max_iterations),
        encode_optional_usize(event.evaluations),
        event
            .best_log_likelihood
            .map(|value| format!("{value:.17}"))
            .unwrap_or_default(),
    ];
    let mut line = fields.join("\t");
    line.push('\n');
    line
}

fn encode_optional_usize(value: Option<usize>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn encode_optional_field(value: Option<&str>) -> String {
    value.map(encode_field).unwrap_or_default()
}

fn encode_field(value: &str) -> String {
    let mut encoded = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'%' | b'\t' | b'\r' | b'\n' => {
                encoded.push(b'%');
                encoded.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            _ => encoded.push(byte),
        }
    }
    String::from_utf8(encoded).expect("progress field encoding preserves UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_event_is_one_self_describing_tsv_line() {
        let line = format_event(
            7,
            ProgressEvent {
                event: "optimization_iteration",
                command: "model-optimize",
                dataset_id: Some("study%1"),
                model_id: Some("DEC\tJ"),
                start: Some(2),
                starts: Some(3),
                iteration: Some(11),
                max_iterations: Some(200),
                evaluations: Some(23),
                best_log_likelihood: Some(-12.5),
                ..ProgressEvent::default()
            },
        );
        assert_eq!(
            line,
            "biogeo-cli-progress-v1\t7\toptimization_iteration\tmodel-optimize\tstudy%251\tDEC%09J\t\t\t2\t3\t11\t200\t23\t-12.50000000000000000\n"
        );
        assert_eq!(line.lines().count(), 1);
    }
}
