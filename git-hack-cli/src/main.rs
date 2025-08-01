use clap::Parser;
use remote_git_dump;
use remote_git_dump::AtomItem;
use std::collections::VecDeque;
use std::fs::{create_dir, create_dir_all};
use tracing::info;

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

fn main() {
    // let filter = EnvFilter::from_default_env() // 支持 RUST_LOG，但也可以 fallback
    //     .add_directive(Level::INFO.into());
    // tracing_subscriber::fmt()
    //     .with_env_filter(filter)
    //     .without_time()
    //     .with_target(false)
    //     .init();
    let args = Cli::parse();

    let app = match remote_git_dump::init_app(&args.url, &args.url) {
        Ok(r) => r,
        Err(e) => {
            panic!("{}", e);
        }
    };

    let time = std::time::Instant::now();

    let index_files_list = app.dump_index();
    assert!(index_files_list.is_ok());
    let index_entries = index_files_list.unwrap();
    for index_entry in index_entries {
        let res = app.dump_blob(index_entry.path, &index_entry.sha1);
    }
    let branches = app.get_branches();
    for branch in branches.unwrap() {
        let mut commit_queue = VecDeque::<String>::new();
        commit_queue.push_back(branch.sha1);

        while let Some(curr_commit_sha1) = commit_queue.pop_front() {
            let result = app.dump_commit(&curr_commit_sha1);

            let result = result.unwrap();
            result.parents_sha1.iter().for_each(|sha1| {
                commit_queue.push_back(sha1.to_string());
            });
            let dump_files_save_path = app
                .store_path
                .join(".remote-git-dump-files")
                .join(format!("{}-commit-{}", branch.name, curr_commit_sha1));
            if !dump_files_save_path.exists() {
                create_dir_all(&dump_files_save_path).unwrap();
            }
            let mut trees = VecDeque::<AtomItem>::new();

            trees.push_back(AtomItem::path(dump_files_save_path, result.tree_sha1));

            while let Some(curr_tree_atom) = trees.pop_front() {
                let dump_tree_result = app.dump_tree(curr_tree_atom.sha1);

                let dump_tree = dump_tree_result.unwrap();
                for tree in dump_tree.trees {
                    let tree_path = curr_tree_atom.path.join(tree.name);
                    if !tree_path.exists() {
                        create_dir(&tree_path).unwrap();
                    }
                    trees.push_back(AtomItem::path(tree_path, tree.sha1))
                }

                for blob in dump_tree.blobs {
                    let save_path = curr_tree_atom.path.join(blob.name);
                    app.dump_blob(save_path, &blob.sha1).unwrap()
                }
            }
        }
    }

    let duration = time.elapsed();

    info!("耗时 {:.2?}", duration);
}
