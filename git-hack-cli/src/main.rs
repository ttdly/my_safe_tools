use clap::Parser;
use git_hack::GitHack;
use tracing::{Level, error, info};
use tracing_subscriber::EnvFilter;
use traits::Application;
use url::Url;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// 目标地址
    #[arg(required = true, short, long)]
    url: String,
    /// 下载文件存储地址
    #[arg(required = false, short, long, default_value = "./temp")]
    path: String,
}
//
// struct PrintlnVisitor {
//     msg: String,
// }
//
// impl Default for PrintlnVisitor {
//     fn default() -> Self {
//         Self {
//             msg: String::new(),
//         }
//     }
// }
//
// impl tracing::field::Visit for PrintlnVisitor {
//     fn record_str(&mut self, field: &Field, value: &str) {
//         if field.name() == "message" {
//             self.msg = value.to_string()
//         }
//     }
//
//     fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
//         if field.name() == "message" {
//             self.msg = format!("{:?}", value);
//         }
//     }
// }
//
// struct CustomLayer;
//
// impl<S> Layer<S> for CustomLayer
// where
//     S: tracing::Subscriber,
// {
//     fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
//
//         let mut visitor:PrintlnVisitor = PrintlnVisitor::default();
//         _event.record(&mut visitor);
//
//         let suffix = match _event.metadata().level() {
//             &Level::WARN => Style::new().yellow().apply_to("[Warn]").to_string(),
//             &Level::ERROR => Style::new().red().apply_to("[Error]").to_string(),
//             _ => "[Debug]".to_string()
//         };
//
//         println!("{} {}", suffix, visitor.msg);
//     }
// }

fn main() {
    let filter = EnvFilter::from_default_env() // 支持 RUST_LOG，但也可以 fallback
        .add_directive(Level::INFO.into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .with_target(false)
        .init();
    let args = Cli::parse();
    match Url::parse(&args.url) {
        Err(_) => {
            error!("不正确的链接： {}", args.url);
        }
        Ok(_) => {}
    }

    let mut app = GitHack::new(&args.url, &args.path);
    let time = std::time::Instant::now();

    app.execute();

    let duration = time.elapsed();

    info!("耗时 {:.2?}", duration);
}
