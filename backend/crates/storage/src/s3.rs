use std::path::Path;

use aws_sdk_s3::{
    config::{Credentials, Region},
    error::SdkError,
    operation::{get_object::GetObjectError, head_object::HeadObjectError},
    primitives::ByteStream,
    Client,
};

use crate::{path::object_key_path, Error, Result};

/// 创建 S3 客户端所需的启动配置。
#[derive(Clone)]
pub struct S3StorageConfig {
    /// 存放对象的 bucket。
    pub bucket: String,
    /// AWS region 或 S3-compatible 服务约定的签名 region。
    pub region: String,
    /// 自定义 endpoint；AWS S3 可留空。
    pub endpoint: Option<String>,
    /// 访问密钥 ID。
    pub access_key_id: String,
    /// 访问密钥。
    pub secret_access_key: String,
    /// 临时凭证的 session token。
    pub session_token: Option<String>,
    /// 所有对象键的可选前缀。
    pub key_prefix: Option<String>,
    /// 是否强制 path-style URL，MinIO 等兼容服务通常需要开启。
    pub force_path_style: bool,
}

/// 基于 AWS SDK 的 S3 对象存储实现。
#[derive(Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    key_prefix: Option<String>,
}

impl S3Storage {
    /// 根据显式凭证和 endpoint 配置创建 S3 存储。
    ///
    /// # 参数
    /// * `config` - bucket、region、凭证、endpoint 和对象键前缀。
    ///
    /// # 返回
    /// 返回可复用的 S3 存储客户端。
    ///
    /// # 错误
    /// bucket、region、凭证为空，endpoint 格式无效，或对象键前缀不安全时返回错误。
    pub fn new(config: S3StorageConfig) -> Result<Self> {
        validate_config(&config)?;

        let credentials = Credentials::new(
            config.access_key_id,
            config.secret_access_key,
            config.session_token,
            None,
            "erp-config",
        );
        let mut sdk_config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(Region::new(config.region))
            .credentials_provider(credentials)
            .force_path_style(config.force_path_style);
        if let Some(endpoint) = config.endpoint {
            sdk_config = sdk_config.endpoint_url(endpoint);
        }

        Ok(Self {
            client: Client::from_conf(sdk_config.build()),
            bucket: config.bucket,
            key_prefix: normalize_prefix(config.key_prefix)?,
        })
    }

    /// 将文件保存到 S3 对象键。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    /// * `content` - 对象内容。
    ///
    /// # 错误
    /// 路径无效或 S3 `PutObject` 失败时返回错误。
    pub async fn save<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> Result<()> {
        let key = self.object_key(path.as_ref())?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(content.to_vec()))
            .send()
            .await
            .map_err(s3_error)?;
        Ok(())
    }

    /// 读取 S3 对象的完整内容。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    ///
    /// # 返回
    /// 返回对象的完整字节内容。
    ///
    /// # 错误
    /// 对象不存在时返回 `Error::NotFound`；路径无效、S3 请求或响应体读取失败时返回错误。
    pub async fn read<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>> {
        let key = self.object_key(path.as_ref())?;
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(get_error)?;
        let body = response.body.collect().await.map_err(s3_error)?;
        Ok(body.into_bytes().to_vec())
    }

    /// 删除 S3 对象。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    ///
    /// # 错误
    /// 对象不存在时返回 `Error::NotFound`；路径无效或 S3 请求失败时返回错误。
    pub async fn delete<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let key = self.object_key(path.as_ref())?;
        if !self.object_exists(&key).await? {
            return Err(Error::NotFound);
        }

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(s3_error)?;
        Ok(())
    }

    /// 检查 S3 对象是否存在。
    ///
    /// 路径无效、鉴权失败或 S3 不可用时均返回 `false`，与现有本地存储合同一致。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    ///
    /// # 返回
    /// 仅当 `HeadObject` 成功时返回 `true`。
    pub async fn exists<P: AsRef<Path>>(&self, path: P) -> bool {
        let Ok(key) = self.object_key(path.as_ref()) else {
            return false;
        };
        self.object_exists(&key).await.unwrap_or(false)
    }

    /// 返回加上可选前缀的规范 S3 对象键。
    fn object_key(&self, path: &Path) -> Result<String> {
        let path = object_key_path(path)?;
        Ok(self
            .key_prefix
            .as_ref()
            .map(|prefix| format!("{prefix}/{path}"))
            .unwrap_or(path))
    }

    /// 通过 `HeadObject` 区分对象不存在与存储服务失败。
    async fn object_exists(&self, key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(error) if head_not_found(&error) => Ok(false),
            Err(error) => Err(s3_error(error)),
        }
    }

    #[cfg(test)]
    /// 用可控 AWS SDK 客户端构造测试实例。
    fn from_client(client: Client, bucket: &str, key_prefix: Option<&str>) -> Result<Self> {
        Ok(Self {
            client,
            bucket: bucket.to_string(),
            key_prefix: normalize_prefix(key_prefix.map(str::to_owned))?,
        })
    }
}

/// 校验建立 S3 签名客户端必需的配置。
fn validate_config(config: &S3StorageConfig) -> Result<()> {
    for (name, value) in [
        ("bucket", config.bucket.as_str()),
        ("region", config.region.as_str()),
        ("access_key_id", config.access_key_id.as_str()),
        ("secret_access_key", config.secret_access_key.as_str()),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(Error::InvalidConfig(format!("S3 {name} 不能为空或包含首尾空白")));
        }
    }

    if config
        .endpoint
        .as_deref()
        .is_some_and(|endpoint| !is_valid_endpoint(endpoint))
    {
        return Err(Error::InvalidConfig(
            "S3 endpoint 必须使用 http:// 或 https:// 绝对地址".to_string(),
        ));
    }
    if config
        .session_token
        .as_ref()
        .is_some_and(|token| token.trim().is_empty() || token.trim() != token)
    {
        return Err(Error::InvalidConfig(
            "S3 session_token 不能为空或包含首尾空白".to_string(),
        ));
    }

    normalize_prefix(config.key_prefix.clone()).map(|_| ())
}

/// 判断 endpoint 是否为带主机部分且不含空白的 HTTP(S) 绝对地址。
fn is_valid_endpoint(endpoint: &str) -> bool {
    let authority_and_path = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"));
    authority_and_path.is_some_and(|value| {
        !value.is_empty()
            && !value.starts_with('/')
            && value.chars().all(|character| !character.is_whitespace())
    })
}

/// 将可选对象键前缀规范为不带首尾分隔符的相对键。
fn normalize_prefix(prefix: Option<String>) -> Result<Option<String>> {
    let Some(prefix) = prefix else {
        return Ok(None);
    };
    if prefix.is_empty()
        || prefix.trim() != prefix
        || prefix.starts_with('/')
        || prefix.ends_with('/')
        || prefix.contains('\\')
        || prefix
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::InvalidConfig(
            "S3 key_prefix 必须是不带首尾分隔符的相对对象键前缀".to_string(),
        ));
    }
    Ok(Some(prefix))
}

/// 将 S3 SDK 错误统一转换为存储错误。
fn s3_error(error: impl std::fmt::Display) -> Error {
    Error::S3(error.to_string())
}

/// 将 `GetObject` 的不存在语义映射到统一存储错误。
fn get_error<R>(error: SdkError<GetObjectError, R>) -> Error {
    if error
        .as_service_error()
        .is_some_and(GetObjectError::is_no_such_key)
    {
        return Error::NotFound;
    }
    s3_error(error)
}

/// 判断 `HeadObject` 失败是否表示对象不存在。
fn head_not_found<R>(error: &SdkError<HeadObjectError, R>) -> bool {
    error
        .as_service_error()
        .is_some_and(HeadObjectError::is_not_found)
}

#[cfg(test)]
mod tests {
    use aws_sdk_s3::{
        config::{Credentials, Region},
        Config,
    };
    use aws_smithy_http_client::test_util::capture_request;

    use super::*;

    /// S3 保存必须使用 bucket、键前缀和跨平台对象键发出 `PutObject`。
    #[tokio::test]
    async fn saves_object_with_configured_prefix() -> Result<()> {
        let (http_client, request_receiver) = capture_request(None);
        let sdk_config = Config::builder()
            .behavior_version_latest()
            .credentials_provider(test_credentials())
            .region(Region::new("us-east-1"))
            .endpoint_url("https://s3.example.com")
            .force_path_style(true)
            .http_client(http_client)
            .build();
        let storage = S3Storage::from_client(
            Client::from_conf(sdk_config),
            "erp-assets",
            Some("tenant-a/uploads"),
        )?;

        storage.save("images/example.png", b"image-bytes").await?;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "PUT");
        assert_eq!(
            request.uri(),
            "https://s3.example.com/erp-assets/tenant-a/uploads/images/example.png?x-id=PutObject"
        );
        assert_eq!(request.body().bytes(), Some(b"image-bytes".as_slice()));
        Ok(())
    }

    /// S3 读取必须从同一 bucket 和键前缀发出 `GetObject`。
    #[tokio::test]
    async fn reads_object_with_configured_prefix() -> Result<()> {
        let (http_client, request_receiver) = capture_request(None);
        let sdk_config = Config::builder()
            .behavior_version_latest()
            .credentials_provider(test_credentials())
            .region(Region::new("us-east-1"))
            .endpoint_url("https://s3.example.com")
            .force_path_style(true)
            .http_client(http_client)
            .build();
        let storage = S3Storage::from_client(
            Client::from_conf(sdk_config),
            "erp-assets",
            Some("tenant-a/uploads"),
        )?;

        let content = storage.read("images/example.png").await?;

        let request = request_receiver.expect_request();
        assert!(content.is_empty());
        assert_eq!(request.method(), "GET");
        assert_eq!(
            request.uri(),
            "https://s3.example.com/erp-assets/tenant-a/uploads/images/example.png?x-id=GetObject"
        );
        Ok(())
    }

    /// S3 对象键不得使用父目录分量越过配置前缀。
    #[tokio::test]
    async fn rejects_parent_directory_object_key() -> Result<()> {
        let (http_client, _) = capture_request(None);
        let sdk_config = Config::builder()
            .behavior_version_latest()
            .credentials_provider(test_credentials())
            .region(Region::new("us-east-1"))
            .http_client(http_client)
            .build();
        let storage = S3Storage::from_client(Client::from_conf(sdk_config), "erp-assets", None)?;

        let result = storage.save("../escaped.txt", b"escaped").await;

        assert!(matches!(result, Err(Error::PathError(_))));
        Ok(())
    }

    /// S3 启动配置必须提供非空 bucket、region 和签名凭证。
    #[test]
    fn rejects_incomplete_s3_config() {
        let result = S3Storage::new(S3StorageConfig {
            bucket: String::new(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key_id: "access-key".to_string(),
            secret_access_key: "secret-key".to_string(),
            session_token: None,
            key_prefix: None,
            force_path_style: false,
        });

        assert!(matches!(result, Err(Error::InvalidConfig(_))));
    }

    /// 返回仅用于捕获 SDK 请求的固定测试凭证。
    fn test_credentials() -> Credentials {
        Credentials::new("test-access-key", "test-secret-key", None, None, "storage-test")
    }
}
