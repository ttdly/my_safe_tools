use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use remote_git_dump;
use remote_git_dump::{AtomItem, RemoteGitDump, RemoteGitHackDumpError};
use std::collections::VecDeque;

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

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("执行错误 {0}")]
    DumpError(#[from] RemoteGitHackDumpError),
    #[error("IO 错误")]
    IoError(#[from] std::io::Error),
}

fn dump(app: &RemoteGitDump, pb: &ProgressBar) -> Result<(), Error> {
    let index_files_list = app.dump_index()?;
    for index_entry in index_files_list {
        pb.set_message(format!("恢复文件 {}...", &index_entry.sha1));
        app.dump_blob(index_entry.path, &index_entry.sha1)?;
    }
    let branches = app.get_branches()?;

    for branch in branches {
        let mut commit_queue = VecDeque::<String>::new();
        pb.set_message(format!("处理分支 {}...", &branch.sha1));
        commit_queue.push_back(branch.sha1);
        while let Some(curr_commit_sha1) = commit_queue.pop_front() {
            pb.set_message(format!("处理提交 {}...", &curr_commit_sha1));
            let result = app.dump_commit(&curr_commit_sha1)?;

            let result = result;
            result.parents_sha1.iter().for_each(|sha1| {
                commit_queue.push_back(sha1.to_string());
            });
            let dump_files_save_path = app
                .store_path
                .join(".remote-git-dump-files")
                .join(format!("{}-commit-{}", branch.name, curr_commit_sha1));
            let mut trees = VecDeque::<AtomItem>::new();

            trees.push_back(AtomItem::path(dump_files_save_path, result.tree_sha1));

            while let Some(curr_tree_atom) = trees.pop_front() {
                pb.set_message(format!("处理目录 {}...", &curr_tree_atom.sha1));
                let dump_tree_result = app.dump_tree(curr_tree_atom.sha1)?;
                let dump_tree = dump_tree_result;
                for tree in dump_tree.trees {
                    let tree_path = curr_tree_atom.path.join(tree.name);
                    trees.push_back(AtomItem::path(tree_path, tree.sha1));
                }

                for blob in dump_tree.blobs {
                    let save_path = curr_tree_atom.path.join(blob.name);
                    pb.set_message(format!("恢复文件 {}...", &blob.sha1));
                    app.dump_blob(save_path, &blob.sha1)?;
                }
            }
        }
    }
    Ok(())
}

fn main() {
    let args = Cli::parse();

    let app = match remote_git_dump::init_app(&args.url, &args.path) {
        Ok(r) => r,
        Err(e) => {
            panic!("{}", e);
        }
    };

    let time = std::time::Instant::now();
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    match dump(&app, &pb) {
        Ok(r) => {
            pb.finish_with_message(format!("文件恢复完成，耗时 {:.2?}", time.elapsed()));
            r
        }
        Err(e) => {
            panic!("{}", e.to_string())
        }
    };
}
