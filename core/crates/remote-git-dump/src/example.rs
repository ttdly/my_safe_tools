use crate::{AtomItem, RemoteGitDump};
use rstest::{fixture, rstest};
use std::collections::VecDeque;
use std::fs::{create_dir, create_dir_all};
use std::path::PathBuf;

#[fixture]
fn app() -> RemoteGitDump {
    let url = "http://192.168.2.2/.git/";
    let path = "/Volumes/MacMiniData/genyu/code/temp/test_git_hack";
    match crate::init_app(url, path) {
        Ok(r) => r,
        Err(e) => {
            panic!("{}", e);
        }
    }
}

#[rstest]
fn dump_index(app: RemoteGitDump) {
    let result = app.dump_index();
    assert!(result.is_ok());
    let blobs = result.unwrap();
    assert!(blobs.len() > 0);
    blobs.iter().for_each(|path_and_sha1| {
        println!("{:?}", &path_and_sha1);
        let result = app.dump_blob(
            PathBuf::new().join(&path_and_sha1.name),
            &path_and_sha1.sha1,
        );
        assert!(result.is_ok());
    })
}

#[rstest]
fn get_branches(app: RemoteGitDump) {
    let result = app.get_branches();
    assert!(result.is_ok());
    for branch in result.unwrap() {
        println!("{:?}", branch);
    }
}

#[rstest]
fn dump_commit(app: RemoteGitDump) {
    let result = app.dump_commit(&String::from("1ac61cfbf9c0f770ba0d3b97198f1ce3378dcfe3"));
    assert!(result.is_ok());
    println!("{:?}", result);
}

#[rstest]
fn dump_tree(app: RemoteGitDump) {
    let result = app.dump_tree(String::from("806429dc04fcbaf07ec2ff73b8aedd45524fd9f8"));
    assert!(result.is_ok());
    let result = result.unwrap();
    println!("trees in this tree");
    for tree in result.trees {
        println!("{:?}", tree);
    }
    println!("blobs in this tree");
    for blob in result.blobs {
        println!("{:?}", blob);
    }
}

#[rstest]
fn dump_blob(app: RemoteGitDump) {
    let result = app.dump_blob(
        PathBuf::new().join("README.md"),
        &"3c45319e0730d7248102521f4ace7057f5ee95b2".to_string(),
    );
    assert!(result.is_ok());
}

#[rstest]
fn dump(app: RemoteGitDump) {
    let index_files_list = app.dump_index();
    assert!(index_files_list.is_ok());
    let index_entries = index_files_list.unwrap();
    for index_entry in index_entries {
        let res = app.dump_blob(index_entry.path, &index_entry.sha1);
        assert!(res.is_ok());
    }
    let branches = app.get_branches();
    assert!(branches.is_ok());
    for branch in branches.unwrap() {
        let mut commit_queue = VecDeque::<String>::new();
        commit_queue.push_back(branch.sha1);

        while let Some(curr_commit_sha1) = commit_queue.pop_front() {
            let result = app.dump_commit(&curr_commit_sha1);
            assert!(result.is_ok());
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

                assert!(dump_tree_result.is_ok());
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
}
