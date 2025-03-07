use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Sender},
        RwLock,
    },
};

use tower_lsp::{lsp_types::MessageType, Client};
use tracing::{
    field,
    span::{self},
    Level, Subscriber,
};

pub struct LspSubscriber {
    tx: Sender<(MessageType, String)>,
    count: RwLock<u64>,
    span_name_map: RwLock<HashMap<span::Id, String>>,
    prefix: RwLock<Vec<String>>,
}

impl LspSubscriber {
    pub fn new(client: Client) -> LspSubscriber {
        let (tx, rx) = mpsc::channel();
        tokio::spawn(async move {
            while let Ok((typ, message)) = rx.recv() {
                client.log_message(typ, message).await;
            }
        });
        LspSubscriber {
            count: RwLock::new(0),
            tx,
            span_name_map: RwLock::new(HashMap::new()),
            prefix: RwLock::new(vec![]),
        }
    }
    fn log(&self, typ: MessageType, message: String) {
        let _ = self.tx.send((typ, message));
    }
}

impl Subscriber for LspSubscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= Level::DEBUG
    }

    fn new_span(&self, span: &span::Attributes<'_>) -> span::Id {
        *self.count.write().unwrap() += 1;
        let mut span_name_map = self.span_name_map.write().unwrap();
        let id = span::Id::from_u64(*self.count.read().unwrap());
        span_name_map.insert(id.clone(), span.metadata().name().to_string());
        id
    }

    fn record(&self, span: &span::Id, values: &span::Record<'_>) {
        self.log(
            MessageType::LOG,
            format!("span: {}, record: {:?}", span.into_u64(), values),
        );
    }

    fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
        self.log(
            MessageType::LOG,
            format!(
                "span: {}, follows: {:?}",
                span.into_u64(),
                follows.into_u64()
            ),
        );
    }

    fn event(&self, event: &tracing::Event<'_>) {
        let typ = match *event.metadata().level() {
            Level::TRACE => return,
            Level::DEBUG => MessageType::LOG,
            Level::INFO => MessageType::INFO,
            Level::WARN => MessageType::WARNING,
            Level::ERROR => MessageType::ERROR,
        };

        let mut logger_visitor = LoggerVisit {
            message: String::new(),
        };
        event.record(&mut logger_visitor);
        let prefix = self.prefix.read().unwrap();
        self.log(
            typ,
            format!("{}{}", prefix.join(""), logger_visitor.message),
        );
    }

    fn enter(&self, id: &span::Id) {
        let mut prefix = self.prefix.write().unwrap();
        let span_name_map = self.span_name_map.read().unwrap();
        prefix.push(format!("{}:", span_name_map.get(id).unwrap()));
    }

    fn exit(&self, _id: &span::Id) {
        let mut prefix = self.prefix.write().unwrap();
        prefix.pop();
    }
}

struct LoggerVisit {
    pub message: String,
}

impl field::Visit for LoggerVisit {
    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        let cur_message = if field.name() == "message" {
            format!("{:?}", value)
        } else {
            format!("{}={:?}", field.name(), value)
        };
        self.message = if self.message.is_empty() {
            cur_message
        } else {
            format!("{},{}", self.message, cur_message)
        }
    }
}
