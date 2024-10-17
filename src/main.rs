use std::sync::mpsc::sync_channel;

use cassandra_cpp::{Cluster, LendingIterator};
use nu_plugin::{serve_plugin, MsgPackSerializer, Plugin, PluginCommand};
use nu_plugin::{EngineInterface, EvaluatedCall};
use nu_protocol::{
    Category, Example, LabeledError, ListStream, PipelineData, Record, Signals, Signature, Span,
    SyntaxShape, Type, Value,
};
use uuid::Uuid;

pub struct CassandraQueryPlugin {
    handle: tokio::runtime::Handle,
}

impl Plugin for CassandraQueryPlugin {
    fn version(&self) -> String {
        // This automatically uses the version of your package from Cargo.toml as the plugin version
        // sent to Nushell
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![
            // Commands should be added here
            Box::new(CassandraQuery),
        ]
    }
}

pub struct CassandraQuery;

impl PluginCommand for CassandraQuery {
    type Plugin = CassandraQueryPlugin;

    fn name(&self) -> &str {
        "cassandra-query"
    }

    fn signature(&self) -> Signature {
        Signature::build(self.name())
            .input_output_type(Type::Nothing, Type::table())
            .required("query", SyntaxShape::String, "CQL query to run")
            .category(Category::Database)
    }

    fn description(&self) -> &str {
        "(FIXME) help text for cassandra-query"
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            example: "cassandra-query `SELECT keyspace_name FROM system_schema.keyspaces`",
            description: "Get list of keyspaces",
            result: None,
        }]
    }

    fn run(
        &self,
        plugin: &CassandraQueryPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: PipelineData,
    ) -> Result<PipelineData, LabeledError> {
        plugin.handle.block_on(run(&call))
    }
}

async fn run(call: &EvaluatedCall) -> Result<PipelineData, LabeledError> {
    let span = call.head;
    let query: String = call.req(0)?;
    let mut cluster = Cluster::default();
    cluster.set_contact_points("127.0.0.1").label(span)?;
    cluster.set_load_balance_round_robin();
    let session = cluster.connect().await.label(span)?;
    let mut statement = session.statement(&query);
    statement.set_paging_size(1024).label(span)?;
    let mut result = session
        .execute_with_payloads(&statement)
        .await
        .label(span)?
        .0;
    let (tx, rx) = sync_channel(1024);

    let columns = (0..result.column_count())
        .map(|idx| result.column_name(idx as usize).map(|s| s.to_owned()))
        .collect::<Result<Vec<String>, _>>()
        .label(span)?;

    tokio::spawn(async move {
        loop {
            let mut iter = result.iter();
            while let Some(row) = iter.next() {
                let mut record = Record::with_capacity(columns.len());
                for (idx, col) in columns.iter().enumerate() {
                    let nu_val = row
                        .get_column(idx)
                        .label(span)
                        .map(|val| get_cassandra_value(val, span))
                        .unwrap_or_else(|err| Value::error(err.into(), span));
                    record.insert(col, nu_val);
                }
                if tx.send(Value::record(record, span)).is_err() {
                    break;
                }
            }
            if let Ok(Some(token)) = result.paging_state_token() {
                if let Err(err) = statement.set_paging_state_token(&token) {
                    let _ = tx.send(Value::error(err.label(span).into(), span));
                    break;
                }
                match session.execute_with_payloads(&statement).await.label(span) {
                    Ok((next_result, _)) => {
                        drop(iter);
                        result = next_result;
                    }
                    Err(err) => {
                        let _ = tx.send(Value::error(err.into(), span));
                        break;
                    }
                }
            } else {
                break;
            }
        }
        drop(session);
        drop(cluster);
    });
    Ok(PipelineData::ListStream(
        ListStream::new(rx.into_iter(), span, Signals::empty()),
        None,
    ))
}

fn get_cassandra_value(val: cassandra_cpp::Value, span: Span) -> Value {
    match val.get_type() {
        cassandra_cpp::ValueType::ASCII => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::BIGINT => val.get_i64().label(span).map(|v| Value::int(v, span)),
        cassandra_cpp::ValueType::BLOB => {
            val.get_bytes().label(span).map(|v| Value::binary(v, span))
        }
        cassandra_cpp::ValueType::BOOLEAN => {
            val.get_bool().label(span).map(|v| Value::bool(v, span))
        }
        cassandra_cpp::ValueType::COUNTER => val.get_i64().label(span).map(|v| Value::int(v, span)),
        cassandra_cpp::ValueType::DECIMAL => val
            .get_decimal()
            .label(span)
            .map(|v| Value::string(v.to_string(), span)),
        cassandra_cpp::ValueType::DOUBLE => {
            val.get_f64().label(span).map(|v| Value::float(v, span))
        }
        cassandra_cpp::ValueType::FLOAT => val
            .get_f32()
            .label(span)
            .map(|v| Value::float(v as f64, span)),
        cassandra_cpp::ValueType::INT => val
            .get_i32()
            .label(span)
            .map(|v| Value::int(v as i64, span)),
        cassandra_cpp::ValueType::TEXT => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::TIMESTAMP => val.get_i64().label(span).map(|v| {
            Value::date(
                chrono::DateTime::from_timestamp_millis(v)
                    .expect("bad date")
                    .fixed_offset(),
                span,
            )
        }),
        cassandra_cpp::ValueType::UUID => val
            .get_bytes()
            .label(span)
            .and_then(|v| Uuid::from_slice(&v).map_err(|err| LabeledError::new(err.to_string())))
            .map(|uuid| Value::string(uuid.to_string(), span)),
        cassandra_cpp::ValueType::VARCHAR => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::VARINT => val.get_i64().label(span).map(|v| Value::int(v, span)),
        cassandra_cpp::ValueType::TIMEUUID => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::INET => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::DATE => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::TIME => {
            val.get_string().label(span).map(|v| Value::string(v, span))
        }
        cassandra_cpp::ValueType::SMALL_INT => val
            .get_i16()
            .label(span)
            .map(|v| Value::int(v as i64, span)),
        cassandra_cpp::ValueType::TINY_INT => {
            val.get_i8().label(span).map(|v| Value::int(v as i64, span))
        }
        cassandra_cpp::ValueType::DURATION => {
            val.get_i64().label(span).map(|v| Value::duration(v, span))
        }
        cassandra_cpp::ValueType::LIST
        | cassandra_cpp::ValueType::SET
        | cassandra_cpp::ValueType::TUPLE => val.get_set().label(span).map(|mut list_iter| {
            let mut list = vec![];
            while let Some(item) = list_iter.next() {
                list.push(get_cassandra_value(item, span));
            }
            Value::list(list, span)
        }),
        cassandra_cpp::ValueType::MAP => val.get_map().label(span).map(|mut map_iter| {
            let mut record = Record::new();
            while let Some((key, val)) = map_iter.next() {
                record.insert(
                    key.get_str().unwrap_or("<error>"),
                    get_cassandra_value(val, span),
                );
            }
            Value::record(record, span)
        }),
        other => Err(LabeledError::new("Unsupported Cassandra type")
            .with_label(format!("{:?}", other), span)),
    }
    .unwrap_or_else(|err| Value::error(err.into(), span))
}

trait LabelCassandraError {
    type Labeled;

    fn label(self, span: Span) -> Self::Labeled;
}

impl LabelCassandraError for cassandra_cpp::Error {
    type Labeled = LabeledError;

    fn label(self, span: Span) -> LabeledError {
        LabeledError::new("Cassandra query error").with_label(format!("{}", self), span)
    }
}

impl<T, E> LabelCassandraError for Result<T, E>
where
    E: LabelCassandraError,
{
    type Labeled = Result<T, E::Labeled>;

    fn label(self, span: Span) -> Self::Labeled {
        self.map_err(|err| err.label(span))
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let handle = runtime.handle().clone();
    runtime
        .block_on(runtime.spawn_blocking(|| {
            serve_plugin(&CassandraQueryPlugin { handle }, MsgPackSerializer);
        }))
        .expect("panic in runtime");
}
