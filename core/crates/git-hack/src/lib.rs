#[cfg(test)]
mod tests;

use bytes::Bytes;
use flate2::read::ZlibDecoder;
use gix_config::parse::Event as GixConfigParseEvent;
use gix_index::decode::Options;
use gix_index::hash::Kind;
use gix_object::bstr::{BStr, ByteSlice};
use gix_object::tree::EntryRef;
use gix_object::{CommitRef, TreeRef};
use pathdiff::diff_paths;
use reqwest::Url;
use reqwest::blocking::Client;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{error, info};
use traits::Application;

struct ObjectEntry {
    sha1: String,
    down_path: String,
}

impl Display for ObjectEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "条目 SHA1：{}\n存储目录：{}", self.sha1, self.down_path,)
    }
}

impl ObjectEntry {
    fn new(sha1: String) -> ObjectEntry {
        // TODO 格式校验
        let down_path = format!("objects/{}/{}", &sha1[0..2], &sha1[2..]);
        ObjectEntry { sha1, down_path }
    }
}

pub struct GitHack {
    base_url: Url,
    store_path: PathBuf,
    repo_path: PathBuf,
    branches: Vec<(String, String)>,

    client: Client,
}

#[derive(thiserror::Error, Debug)]
pub enum GitHackError {
    #[error("reqwest Url 解析错误 {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("reqwest 请求出错 {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Http 返回错误 {0}")]
    HTTPError(String),

    #[error("IO 错误 {0}")]
    IOError(#[from] std::io::Error),

    #[error("文件地址为空或没有父目录")]
    PathParseError,

    #[error("解析 config 文件出错 {0}")]
    GixConfigParseError(#[from] gix_config::parse::Error),

    #[error("字节流转换失败 {0}")]
    Bytes2Utf8StringError(#[from] std::string::FromUtf8Error),

    #[error("解析对象文件出错 {0}")]
    GixObjectParseError(#[from] gix_object::decode::Error),

    #[error("未能正确处理解压后的对象数据")]
    HandleDecodedObjectError,

    #[error("索引文件初始化失败 {0}")]
    IndexParseError(#[from] gix_index::file::init::Error),
}

impl GitHack {
    // public
    pub fn new(base_url: &str, store_path: &str) -> Self {
        let base_url = Url::parse(base_url).expect("无法解析输入的 URL"); // TODO
        let store_path = PathBuf::from(store_path);
        let repo_path = store_path.join(".git");
        fs::create_dir_all(&repo_path).unwrap();
        let repo_path = repo_path.canonicalize().expect("获取绝对路径失败");
        let store_path = store_path.canonicalize().expect("获取绝对路径失败");
        GitHack {
            base_url,
            store_path,
            repo_path,
            branches: vec![],
            client: Client::new(),
        }
    }

    /// 通过检查目标地址的 index 文件来确认目标目录是否是 git 仓库
    pub fn check(&mut self) -> Result<(), GitHackError> {
        let _ = self.download("index")?;
        Ok(())
    }

    /// 下载目标地址中的 config 文件并通过解析获取仓库中的所有分支名以及对应的 hash
    pub fn get_repo_branches(&mut self) -> Result<(), GitHackError> {
        let config_content_byte = self.download("config")?;
        let mut branches_name: Vec<String> = Vec::new();
        let mut dispatch = |event: GixConfigParseEvent| {
            let GixConfigParseEvent::SectionHeader(section) = event else {
                return;
            };
            if !section.name().eq(BStr::new("branch")) {
                return;
            };
            // 这里使用 unwarp 是因为 branch 一定有 subsection 这里是安全的
            branches_name.push(section.subsection_name().unwrap().to_string());
        };
        gix_config::parse::from_bytes(config_content_byte.as_bytes(), &mut dispatch)?;
        branches_name
            .iter()
            .try_for_each(|name| -> Result<(), GitHackError> {
                let ref_sha1 = self.download(format!("refs/heads/{}", &name).as_str())?;
                self.branches.push((
                    name.to_string(),
                    String::from_utf8(ref_sha1.to_vec())?.trim_end().to_string(),
                ));
                Ok(())
            })?;
        Ok(())
    }

    /// 恢复所有能获取的信息
    ///
    /// 需要在获取仓库分支名和 sha1 之后才能执行，否则无法获取任何信息。
    pub fn dump(&mut self) -> Result<(), GitHackError> {
        self.dump_index()?;
        self.branches.iter().try_for_each(
            |(name, sha1): &(String, String)| -> Result<(), GitHackError> {
                self.dump_commit(name, sha1)?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn exclude_dump_dir(&self) -> Result<(), GitHackError> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.store_path.join(".gitignore"))?;
        file.write_all(b".git-hack-dump")?;
        Ok(())
    }

    fn dump_index(&self) -> Result<(), GitHackError> {
        info!("恢复索引文件...");
        let index_path = self.repo_path.join("index");
        let index = gix_index::File::at(index_path, Kind::Sha1, false, Options::default())?;
        index
            .entries()
            .iter()
            .try_for_each(|entry| -> Result<(), GitHackError> {
                let file_path = entry.path(&index).to_string();
                let target_path = self.store_path.join(&file_path);
                let Some(target_path_parent) = target_path.parent() else {
                    return Err(GitHackError::PathParseError);
                };
                if !target_path_parent.exists() {
                    fs::create_dir_all(target_path_parent)?;
                }

                self.dump_blob(target_path, entry.id.to_string())?;
                Ok(())
            })?;
        info!("已存储索引文件中的所有对象");
        Ok(())
    }

    /// 恢复提交
    fn dump_commit(&self, branch_name: &String, latest_sha1: &String) -> Result<(), GitHackError> {
        let mut commit_sha1_queue: VecDeque<String> = VecDeque::new();
        commit_sha1_queue.push_back(latest_sha1.to_owned());

        while let Some(curr_commit_sha1) = commit_sha1_queue.pop_front() {
            let object_entry: ObjectEntry = ObjectEntry::new(curr_commit_sha1.to_string());
            let object_bytes: Bytes = self.download(&object_entry.down_path)?;
            let object_bytes: Vec<u8> = self.read_object_from_bytes(object_bytes)?;
            let commit: CommitRef = CommitRef::from_bytes(object_bytes.as_slice())?;

            // 将父 sha1 添加到队列中
            commit
                .parents
                .iter()
                .try_for_each(|parent: &&BStr| -> Result<(), GitHackError> {
                    commit_sha1_queue.push_back(parent.to_string());
                    Ok(())
                })?;

            // 处理 tree 对象
            let dump_file_save_path: PathBuf = self
                .store_path
                .join(".git-hack-dump")
                .join(format!("{}.{}", branch_name, curr_commit_sha1));
            if !fs::exists(dump_file_save_path.as_path())? {
                fs::create_dir_all(&dump_file_save_path)?;
            }
            self.dump_tree(dump_file_save_path, commit.tree.to_string())?;
            info!("[commit] {}", &curr_commit_sha1);
        }
        Ok(())
    }

    /// 恢复目录
    fn dump_tree(&self, save_path: PathBuf, tree_sha1: String) -> Result<(), GitHackError> {
        let mut tree_sha1_queue: VecDeque<(PathBuf, String)> = VecDeque::new();
        tree_sha1_queue.push_back((save_path, tree_sha1));

        while let Some(curr_tree_sha1) = tree_sha1_queue.pop_front() {
            let object_entry: ObjectEntry = ObjectEntry::new(curr_tree_sha1.1);
            let object_bytes: Bytes = self.download(&object_entry.down_path)?;
            let object_bytes: Vec<u8> = self.read_object_from_bytes(object_bytes)?;
            let tree: TreeRef = TreeRef::from_bytes(object_bytes.as_slice())?;

            tree.entries
                .iter()
                .try_for_each(|entry: &EntryRef| -> Result<(), GitHackError> {
                    if entry.mode.is_tree() {
                        let tree_path: PathBuf = curr_tree_sha1.0.join(entry.filename.to_string());
                        if !tree_path.exists() {
                            fs::create_dir(&tree_path)?;
                        }
                        tree_sha1_queue.push_back((tree_path, entry.oid.to_string()));
                    }

                    if entry.mode.is_blob() {
                        self.dump_blob(
                            curr_tree_sha1.0.join(entry.filename.to_string()),
                            entry.oid.to_string(),
                        )?;
                    }
                    Ok(())
                })?;
            info!("[tree] {}", &object_entry.sha1);
        }
        Ok(())
    }

    /// 恢复文件
    fn dump_blob(&self, save_path: PathBuf, blob_sha1: String) -> Result<(), GitHackError> {
        let object_entry: ObjectEntry = ObjectEntry::new(blob_sha1);
        let object_bytes: Bytes = self.download(&object_entry.down_path)?;
        let object_bytes: Vec<u8> = self.read_object_from_bytes(object_bytes)?;
        fs::write(&save_path, &object_bytes)?;

        info!(
            "[blob] {}",
            diff_paths(&save_path, &self.store_path)
                .unwrap_or(PathBuf::default())
                .to_str()
                .unwrap_or("unknown")
        );
        Ok(())
    }

    /// 从 reqwest 返回的字节流中解压对象并输出字节流
    fn read_object_from_bytes(&self, object_bytes: Bytes) -> Result<Vec<u8>, GitHackError> {
        let mut decoder: ZlibDecoder<&[u8]> = ZlibDecoder::new(&object_bytes[..]);
        let mut buffer: Vec<u8> = Vec::new();
        decoder.read_to_end(&mut buffer)?;
        // 去除掉开头的 <type> <length>
        let null_pos: usize = buffer
            .iter()
            .position(|&b: &_| b == 0)
            .ok_or_else(|| GitHackError::HandleDecodedObjectError)?;
        Ok(buffer[null_pos + 1..].to_vec())
    }

    /// 通用下载器
    ///
    /// 成功执行的话返回数据为下载文件的字节流
    fn download(&self, file_name: &str) -> Result<Bytes, GitHackError> {
        let target_url = self.base_url.join(file_name)?;
        let target_file_path = self.repo_path.join(file_name);
        let target_file_path = Path::new(&target_file_path);
        let target_dir = match target_file_path.parent() {
            Some(dir) => dir,
            None => return Err(GitHackError::PathParseError),
        };

        if !target_dir.exists() {
            fs::create_dir_all(target_dir)?;
        }

        let response = self.client.get(target_url).send()?;
        if response.status().is_success() {
            let content = response.bytes()?;
            fs::write(target_file_path, &content)?;
            Ok(content)
        } else {
            Err(GitHackError::HTTPError(response.status().to_string()))
        }
    }
}

impl Application for GitHack {
    fn execute(&mut self) {
        info!("GitHack 运行中");
        match self.check() {
            Ok(_) => {
                info!("目标地址存在可下载的 git 文件夹，执行中");
            }
            Err(e) => {
                error!("未检测到仓库: {}", e);
            }
        }

        match self.get_repo_branches() {
            Ok(_) => {
                info!("检测到该 git 仓库有分支 {} 个", self.branches.len());
            }
            Err(e) => {
                error!("获取分支名错误: {}", e)
            }
        }

        match self.dump() {
            Ok(_) => {
                info!("该仓库上所有分支以及其对象下载并 dump 成功");
            }
            Err(e) => {
                error!("dump 分支对象错误: {}", e)
            }
        }

        match self.exclude_dump_dir() {
            Ok(_) => {
                info!("已排除分支 dump 文件");
            }
            Err(e) => {
                error!("无法排除分支 dump 文件夹: {}", e)
            }
        }

        info!("执行成功，所有文件均存放在 {:?} 目录中", self.store_path);
    }
}
