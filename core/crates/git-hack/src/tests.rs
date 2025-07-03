use crate::GitHack;
use rstest::{fixture, rstest};
use tracing_test::traced_test;

#[fixture]
fn git_hack() -> GitHack {
    GitHack::new(
        "http://192.168.2.2/.git/",
        "/Volumes/MacMiniData/genyu/code/temp/test_git_hack",
    )
}

#[traced_test]
#[rstest]
fn down_file(git_hack: GitHack) {
    git_hack.download("config").unwrap();
}

#[traced_test]
#[rstest]
fn dump_index(git_hack: GitHack) {
    git_hack.dump_index().unwrap()
}

#[traced_test]
#[rstest]
fn get_branches_name(mut git_hack: GitHack) {
    let result = git_hack.get_repo_branches();
    assert!(result.is_ok());
    match result {
        Err(err) => println!("{:?}", &err),
        _ => {
            println!("Branches:{:?}", git_hack.branches);
        }
    };
}

#[traced_test]
#[rstest]
fn dump(mut git_hack: GitHack) {
    git_hack.branches = vec![(
        String::from("main"),
        String::from("1ac61cfbf9c0f770ba0d3b97198f1ce3378dcfe3"),
    )];
    let result = git_hack.dump();
    match result {
        Err(err) => println!("{}", &err),
        Ok(_) => {}
    }
}
