use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{self, FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

/// tracing 本身只有五个级别；控制台也明确显示第六级，隐藏内部传输字段。
pub(crate) struct ConsoleFormat;

impl<S, N> FormatEvent<S, N> for ConsoleFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        if super::event_level(event.metadata()) != 5 {
            return fmt::format().format_event(ctx, writer, event);
        }
        let mut visitor = super::JsonVisitor::default();
        event.record(&mut visitor);
        if let Some(scope) = ctx.event_scope() {
            visitor.insert_scopes(
                scope
                    .from_root()
                    .filter_map(|span| {
                        let fields = span.extensions();
                        let fields = fields.get::<super::SpanLogFields>()?;
                        Some(serde_json::json!({ "name": fields.name, "fields": fields.fields }))
                    })
                    .collect(),
            );
        }
        writeln!(
            writer,
            "{} CRITICAL {}: {}",
            chrono::Utc::now().to_rfc3339(),
            event.metadata().target(),
            visitor.into_content()
        )
    }
}
