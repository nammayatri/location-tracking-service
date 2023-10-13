use std::collections::BTreeMap;
use tracing_subscriber::Layer;

pub struct CustomJsonLayer;

impl<S> Layer<S> for CustomJsonLayer
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let span = ctx.span(id).unwrap();
        let mut fields = BTreeMap::new();
        let mut visitor = JsonVisitor(&mut fields);
        attrs.record(&mut visitor);
        let storage = CustomFieldStorage(fields);
        let mut extensions = span.extensions_mut();
        extensions.insert(storage);
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Get the span whose data is being recorded
        let span = ctx.span(id).unwrap();

        // Get a mutable reference to the data we created in new_span
        let mut extensions_mut = span.extensions_mut();
        let custom_field_storage: &mut CustomFieldStorage =
            extensions_mut.get_mut::<CustomFieldStorage>().unwrap();
        let json_data: &mut BTreeMap<String, serde_json::Value> = &mut custom_field_storage.0;

        // And add to using our old friend the visitor!
        let mut visitor = JsonVisitor(json_data);
        values.record(&mut visitor);
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // All of the span context
        let mut spans = vec![];
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                if let Some(storage) = extensions.get::<CustomFieldStorage>() {
                    let field_data: &BTreeMap<String, serde_json::Value> = &storage.0;
                    spans.push(serde_json::json!({
                        "target": span.metadata().target(),
                        "name": span.name(),
                        "level": format!("{:?}", span.metadata().level()),
                        "fields": field_data,
                    }));
                }
            }
        }

        // The fields of the event
        let mut fields = BTreeMap::new();
        let mut visitor = JsonVisitor(&mut fields);
        event.record(&mut visitor);

        // {
        //     "log": "2023-10-04T13:18:40.176429415Z UTC INFO> @beckn-location-tracking-service-master-60f74c-9b784bcf9-gg5zh [requestId-9d11f67a-e5d3-4340-9023-753f3006bdd5] |> lts:dl:processing:7f7896dd-787e-4a0b-8675-e9e6fe93bb8f:a3700864-cfda-40dd-aa94-974a8f6d5339:karnataka.json",
        //     "cluster": "eks-oracle",
        //     "tags": "[]",
        //     "file": "shared-0.1.0 | crates/location_tracking_service/src/redis/commands.rs:385 | location_tracking_service::redis::commands"
        // }

        println!("{:?} ::: {:?}", fields, spans);

        let log = format!(
            "{} {}> {} [requestId-{}] |> {}",
            chrono::Utc::now(),
            event.metadata().level(),
            fields.get("hostname").unwrap_or(&serde_json::json!("")),
            fields.get("request_id").unwrap_or(&serde_json::json!("")),
            fields.get("message").unwrap_or(&serde_json::json!(""))
        );

        let file = format!(
            "{}:{} | {} > {}",
            event.metadata().file().unwrap_or(""),
            event.metadata().line().unwrap_or(0),
            event.metadata().module_path().unwrap_or(""),
            event.metadata().target()
        );

        // Output the event in JSON
        let output = serde_json::json!({
            "log": log,
            "tags": fields.get("tags").unwrap_or(&serde_json::json!([])),
            "file": file,
            "fields": fields,
            "spans": spans,
        });

        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    }
}

struct JsonVisitor<'a>(&'a mut BTreeMap<String, serde_json::Value>);

impl<'a> tracing::field::Visit for JsonVisitor<'a> {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.0.insert(
            field.name().to_string(),
            serde_json::json!(value.to_string()),
        );
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(
            field.name().to_string(),
            serde_json::json!(format!("{:?}", value)),
        );
    }
}

#[derive(Debug)]
struct CustomFieldStorage(BTreeMap<String, serde_json::Value>);
