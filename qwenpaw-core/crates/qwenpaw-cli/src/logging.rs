use std::sync::{Arc, Mutex};

use qwenpaw_app_server::BackendLog;
use tracing::{Event, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::time::{FormatTime as _, SystemTime};
use tracing_subscriber::fmt::writer::{EitherWriter, MakeWriterExt as _};
use tracing_subscriber::fmt::{FmtContext, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

pub(super) fn init() -> Arc<Mutex<Option<BackendLog>>> {
    let destination = Arc::new(Mutex::new(None::<BackendLog>));
    let log = destination.clone();
    let file_writer = move || match log.lock().expect("backend log destination lock").clone() {
        Some(file) => EitherWriter::A(file),
        None => EitherWriter::B(std::io::sink()),
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(false)
        .event_format(ConsoleLogFormat)
        .with_writer(std::io::stderr.and(file_writer))
        .init();
    destination
}

struct ConsoleLogFormat;

#[cfg(test)]
#[path = "logging_browser_tests.rs"]
mod browser_tests;

impl<S, N> FormatEvent<S, N> for ConsoleLogFormat
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        SystemTime.format_time(&mut writer)?;
        let level = if *event.metadata().level() == tracing::Level::WARN {
            "WARNING"
        } else {
            event.metadata().level().as_str()
        };
        write!(writer, " {level} {}: ", event.metadata().target())?;
        if let Some(scope) = context.event_scope() {
            for span in scope.from_root() {
                write!(writer, "{}", span.name())?;
                let extensions = span.extensions();
                let fields = extensions
                    .get::<FormattedFields<N>>()
                    .expect("formatting layer records span fields");
                if !fields.is_empty() {
                    write!(writer, "{{{fields}}}")?;
                }
                write!(writer, ": ")?;
            }
        }
        context
            .field_format()
            .format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn formats_plain_warning_and_other_levels_for_the_original_debug_filters() {
        let capture = Capture(Arc::new(Mutex::new(Vec::new())));
        let output = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .event_format(ConsoleLogFormat)
            .with_writer(move || output.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!("debug fixture");
            tracing::info!("info fixture");
            tracing::warn!("warning fixture");
            tracing::error!("error fixture");
        });
        let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        assert!(!text.contains('\u{001b}'));
        let lines = text
            .lines()
            .map(|line| line.split_once(' ').unwrap().1)
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "DEBUG qwenpaw_core::logging::tests: debug fixture",
                "INFO qwenpaw_core::logging::tests: info fixture",
                "WARNING qwenpaw_core::logging::tests: warning fixture",
                "ERROR qwenpaw_core::logging::tests: error fixture",
            ]
        );
    }

    #[test]
    fn retains_nested_span_context_and_recorded_fields() {
        let capture = Capture(Arc::new(Mutex::new(Vec::new())));
        let output = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .event_format(ConsoleLogFormat)
            .with_writer(move || output.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let request = tracing::info_span!("request", id = 7, status = tracing::field::Empty);
            let _request = request.enter();
            request.record("status", "running");
            let _turn = tracing::info_span!("turn").entered();
            tracing::warn!(attempt = 2, "retry fixture");
        });
        let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        assert_eq!(
            text.trim_end().split_once(' ').unwrap().1,
            "WARNING qwenpaw_core::logging::tests: request{id=7 status=\"running\"}: turn: retry fixture attempt=2"
        );
    }
}
