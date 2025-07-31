use RemoteGitHackDumpError as RGDError;
#[cfg(test)]
mod example;

use bytes::Bytes;
use flate2::read::ZlibDecoder;
use gix_config::parse::Event as GixConfigParseEvent;
use gix_index::decode::Options;
use gix_index::hash::Kind;
use gix_object::bstr::{BStr, ByteSlice};
use gix_object::{CommitRef, TreeRef};
use reqwest::Url;
use reqwest::blocking::{Client, Response};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{error, info};

#[derive(Debug)]
pub struct RemoteGitDump {
    base_url: Url,
    pub store_path: PathBuf,
    repo_path: PathBuf,
    client: Client,
}
#[derive(Debug)]
pub struct AtomItem {
    pub name: String,
    pub sha1: String,
    pub path: PathBuf,
}
#[derive(Debug)]
pub struct DumpCommitResult {
    pub parents_sha1: Vec<String>,
    pub tree_sha1: String,
}
#[derive(Debug)]
pub struct DumpTreeResult {
    pub blobs: Vec<AtomItem>,
    pub trees: Vec<AtomItem>,
}

impl AtomItem {
    pub fn name(name: String, sha1: String) -> AtomItem {
        AtomItem {
            name,
            sha1,
            path: PathBuf::default(),
        }
    }

    pub fn path(path: PathBuf, sha1: String) -> AtomItem {
        AtomItem {
            name: String::default(),
            sha1,
            path,
        }
    }
}

impl Default for RemoteGitDump {
    fn default() -> Self {
        Self {
            base_url: Url::parse("http://default").unwrap(),
            store_path: PathBuf::default(),
            repo_path: PathBuf::default(),
            client: Client::default(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RemoteGitHackDumpError {
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
    #[error("该地址不存在 Git 仓库")]
    RepoNotExists,
    #[error("Sha1 长度不对，标准长度 40，实际长度 {0}")]
    SHA1Error(usize),
}
fn read_object_from_bytes(object_bytes: Bytes) -> Result<Vec<u8>, RemoteGitHackDumpError> {
    let mut decoder: ZlibDecoder<&[u8]> = ZlibDecoder::new(&object_bytes[..]);
    let mut buffer: Vec<u8> = Vec::new();
    decoder.read_to_end(&mut buffer)?;
    // 去除掉开头的 <type> <length>
    let null_pos: usize = buffer
        .iter()
        .position(|&b: &_| b == 0)
        .ok_or_else(|| RemoteGitHackDumpError::HandleDecodedObjectError)?;
    Ok(buffer[null_pos + 1..].to_vec())
}
fn create_path_from_sha1(sha1: &str) -> Result<String, RemoteGitHackDumpError> {
    if sha1.len() != 40 {
        return Err(RGDError::SHA1Error(sha1.len()));
    }
    let (dir_part, file_part) = sha1.split_at(2);
    Ok(format!("objects/{}/{}", dir_part, file_part))
}

pub fn init_app(base_url: &str, store_path: &str) -> Result<RemoteGitDump, RemoteGitHackDumpError> {
    let mut base_url = Url::parse(base_url)?;
    let client = Client::new();
    let response = client.get(base_url.join("index")?).send()?;
    if !response.status().is_success() {
        let response = client.head(base_url.join(".git/index")?).send()?;
        if !response.status().is_success() {
            return Err(RGDError::RepoNotExists);
        } else {
            base_url = base_url.join(".git")?;
        }
    }
    let store_path = PathBuf::from(store_path);
    let repo_path = store_path.join(".git");
    fs::create_dir_all(&repo_path)?;
    let repo_path = repo_path.canonicalize()?;
    let store_path = store_path.canonicalize()?;
    Ok(RemoteGitDump {
        base_url,
        store_path,
        repo_path,
        client,
    })
}
fn check_and_create_parent(path_buf: &PathBuf) -> Result<(), RemoteGitHackDumpError> {
    match path_buf.parent() {
        Some(parent) => {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
            Ok(())
        }
        None => Err(RGDError::PathParseError),
    }
}

impl RemoteGitDump {
    fn read_object_from_bytes(
        &self,
        object_bytes: Bytes,
    ) -> Result<Vec<u8>, RemoteGitHackDumpError> {
        let mut decoder: ZlibDecoder<&[u8]> = ZlibDecoder::new(&object_bytes[..]);
        let mut buffer: Vec<u8> = Vec::new();
        decoder.read_to_end(&mut buffer)?;
        // 去除掉开头的 <type> <length>
        let null_pos: usize = buffer
            .iter()
            .position(|&b: &_| b == 0)
            .ok_or_else(|| RemoteGitHackDumpError::HandleDecodedObjectError)?;
        Ok(buffer[null_pos + 1..].to_vec())
    }
    fn download(&self, file_name: &str) -> Result<bytes::Bytes, RemoteGitHackDumpError> {
        let target_url: Url = self.base_url.join(file_name)?;
        let target_file_path: PathBuf = self.repo_path.join(file_name);

        check_and_create_parent(&target_file_path)?;
        let response: Response = self.client.get(target_url).send()?;
        if response.status().is_success() {
            let content = response.bytes()?;
            fs::write(&target_file_path, &content)?;
            info!("downloaded to {}", target_file_path.display());
            Ok(content)
        } else {
            Err(RemoteGitHackDumpError::HTTPError(
                response.status().to_string(),
            ))
        }
    }
    pub fn get_branches(&self) -> Result<Vec<AtomItem>, RemoteGitHackDumpError> {
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
        let mut result: Vec<AtomItem> = Vec::<AtomItem>::new();
        gix_config::parse::from_bytes(config_content_byte.as_bytes(), &mut dispatch)?;
        branches_name
            .iter()
            .try_for_each(|name| -> Result<(), RemoteGitHackDumpError> {
                let ref_sha1 = self.download(format!("refs/heads/{}", &name).as_str())?;
                result.push(AtomItem::name(
                    name.to_string(),
                    String::from_utf8(ref_sha1.to_vec())?.trim_end().to_string(),
                ));
                Ok(())
            })?;
        Ok(result)
    }
    /// 解析索引文件
    ///
    /// 返回 NameAndSHA1 数组其中 name 是文件的相对路径
    pub fn dump_index(&self) -> Result<Vec<AtomItem>, RemoteGitHackDumpError> {
        let index_path = &self.repo_path.join("index");
        if !index_path.exists() {
            self.download("index")?;
        }
        let index = gix_index::File::at(index_path, Kind::Sha1, false, Options::default())?;
        let mut result = Vec::<AtomItem>::new();
        index
            .entries()
            .iter()
            .try_for_each(|entry| -> Result<(), RemoteGitHackDumpError> {
                let path = PathBuf::from(entry.path(&index).to_string());
                let sha1 = entry.id.to_string();

                result.push(AtomItem::path(path, sha1));
                Ok(())
            })?;
        info!("已存储索引文件中的所有对象");
        Ok(result)
    }
    pub fn dump_blob(&self, path: PathBuf, sha1: &String) -> Result<(), RemoteGitHackDumpError> {
        let object_bytes: Bytes = self.download(create_path_from_sha1(&sha1)?.as_str())?;
        let object_bytes: Vec<u8> = read_object_from_bytes(object_bytes)?;
        let save_path = &self.store_path.join(path);
        info!("dump blob {} to {}", sha1, save_path.display());
        check_and_create_parent(save_path)?;
        fs::write(save_path, &object_bytes)?;
        Ok(())
    }
    pub fn dump_commit(
        &self,
        commit_sha1: &String,
    ) -> Result<DumpCommitResult, RemoteGitHackDumpError> {
        let object_bytes: Bytes = self.download(create_path_from_sha1(&commit_sha1)?.as_str())?;
        let object_bytes: Vec<u8> = self.read_object_from_bytes(object_bytes)?;
        let commit_ref: CommitRef = CommitRef::from_bytes(object_bytes.as_slice())?;
        let parents_sha1: Vec<String> = commit_ref
            .parents
            .iter()
            .map(|parent| parent.to_string())
            .collect();
        let tree_sha1: String = commit_ref.tree.to_string();
        Ok(DumpCommitResult {
            parents_sha1,
            tree_sha1,
        })
    }
    pub fn dump_tree(&self, tree_sha1: String) -> Result<DumpTreeResult, RemoteGitHackDumpError> {
        let object_bytes: Bytes = self.download(create_path_from_sha1(&tree_sha1)?.as_str())?;
        let object_bytes: Vec<u8> = self.read_object_from_bytes(object_bytes)?;
        let tree_ref: TreeRef = match TreeRef::from_bytes(object_bytes.as_slice()) {
            Ok(tree_ref) => tree_ref,
            Err(e) => {
                println!("{}", e);
                panic!("pp")
            }
        };
        let mut blobs: Vec<AtomItem> = Vec::<AtomItem>::new();
        let mut trees: Vec<AtomItem> = Vec::<AtomItem>::new();
        tree_ref
            .entries
            .iter()
            .try_for_each(|entry| -> Result<(), RemoteGitHackDumpError> {
                if entry.mode.is_tree() {
                    trees.push(AtomItem::name(
                        entry.filename.to_string(),
                        entry.oid.to_string(),
                    ));
                }
                if entry.mode.is_blob() {
                    blobs.push(AtomItem::name(
                        entry.filename.to_string(),
                        entry.oid.to_string(),
                    ));
                }
                Ok(())
            })?;
        Ok(DumpTreeResult { blobs, trees })
    }
    pub fn exclude_dump_dir(&self) -> Result<(), RemoteGitHackDumpError> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.store_path.join(".gitignore"))?;
        file.write_all(b".remote-git-dump-dump")?;
        Ok(())
    }
}
