use std::sync::{Arc, Mutex};
use tracing::{Event, info, subscriber::set_global_default};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, layer::Context, registry::Registry};

// 内存收集器层
#[derive(Clone)]
struct MemoryLayer {
    logs: Arc<Mutex<Vec<String>>>, // 修正：正确的 Arc<Mutex<T>> 写法
}

impl MemoryLayer {
    fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }
}

impl<S> Layer<S> for MemoryLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = LogVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        let metadata = event.metadata();
        let level = metadata.level().as_str();

        self.logs
            .lock()
            .unwrap()
            .push(format!("[{}] {}", level, visitor.message));
    }
}

pub struct LogVisitor {
    message: String,
}

impl LogVisitor {
    pub fn boot() {
        let memory_layer = MemoryLayer::new();

        // 设置全局订阅器
        let subscriber = Registry::default().with(memory_layer.clone());
        set_global_default(subscriber).expect("无法全局订阅");
    }

    pub fn show_all() {
        // let logs = memory_layer.get_logs();
        // for log in logs {
        //     println!("{}", log);
        // }
    }
}

impl tracing::field::Visit for LogVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}
