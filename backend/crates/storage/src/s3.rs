use std::path::Path;
use std::time::Duration;

use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use url::Url;

use crate::path::object_key_path;
use crate::{Error, Result};

/// 分片序号下限（S3 取值 1 起）。
const MIN_PART_NUMBER: i32 = 1;
/// 分片序号上限（S3 取值最大 10000）。
const MAX_PART_NUMBER: i32 = 10_000;

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
    /// 对外返回的 CDN 或 bucket 公开访问基础 URL。
    pub public_base_url: String,
    /// 是否强制 path-style URL，MinIO 等兼容服务通常需要开启。
    pub force_path_style: bool,
}

impl S3StorageConfig {
    /// 由必填 bucket、签名与访问参数构造 S3 启动配置。
    ///
    /// # 参数
    /// * `bucket` - 存放对象的 bucket
    /// * `region` - 签名 region
    /// * `access_key_id` - 访问密钥 ID
    /// * `secret_access_key` - 访问密钥
    /// * `public_base_url` - 对外返回的公开访问基础 URL
    ///
    /// # 返回
    /// 返回端点、前缀与 path-style 均为空/关闭的启动配置。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        public_base_url: impl Into<String>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            region: region.into(),
            endpoint: None,
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
            key_prefix: None,
            public_base_url: public_base_url.into(),
            force_path_style: false,
        }
    }

    /// 设置自定义 endpoint。
    ///
    /// # 参数
    /// * `endpoint` - 自定义 endpoint
    ///
    /// # 返回
    /// 返回更新后的启动配置。
    ///
    /// # 错误
    /// 无。
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// 设置临时凭证的 session token。
    ///
    /// # 参数
    /// * `session_token` - 临时凭证的 session token
    ///
    /// # 返回
    /// 返回更新后的启动配置。
    ///
    /// # 错误
    /// 无。
    pub fn with_session_token(mut self, session_token: impl Into<String>) -> Self {
        self.session_token = Some(session_token.into());
        self
    }

    /// 设置对象键前缀。
    ///
    /// # 参数
    /// * `key_prefix` - 对象键前缀
    ///
    /// # 返回
    /// 返回更新后的启动配置。
    ///
    /// # 错误
    /// 无。
    pub fn with_key_prefix(mut self, key_prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(key_prefix.into());
        self
    }

    /// 设置是否强制 path-style URL。
    ///
    /// # 参数
    /// * `force_path_style` - 是否强制 path-style URL
    ///
    /// # 返回
    /// 返回更新后的启动配置。
    ///
    /// # 错误
    /// 无。
    pub fn with_force_path_style(mut self, force_path_style: bool) -> Self {
        self.force_path_style = force_path_style;
        self
    }
}

/// 基于 AWS SDK 的 S3 对象存储实现。
#[derive(Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    key_prefix: Option<String>,
    public_base_url: Url,
}

/// 已直传分片的序号与 ETag，用于合并分片上传。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedPart {
    /// 分片序号（1 起）。
    pub part_number: i32,
    /// 分片 PUT 响应 `ETag` 头（可带引号，合并时自动去除）。
    pub etag: String,
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
        let (public_base_url, key_prefix) = validate_config(&config)?;

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
            key_prefix,
            public_base_url,
        })
    }

    /// 将文件与已校验的 MIME 类型保存到 S3 对象键。
    ///
    /// 大文件改走分片上传路径（见 `create_multipart_upload`），避免全量常驻内存。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    /// * `content` - 对象内容。
    /// * `content_type` - 可选的 HTTP Content-Type。
    ///
    /// # 返回
    /// 成功时返回空。
    ///
    /// # 错误
    /// 路径无效或 S3 `PutObject` 失败时返回错误。
    pub async fn save_with_content_type<P: AsRef<Path>>(
        &self,
        path: P,
        content: &[u8],
        content_type: Option<&str>,
    ) -> Result<()> {
        let key = self.object_key(path.as_ref())?;
        let mut request =
            self.client.put_object().bucket(&self.bucket).key(key).body(ByteStream::from(content.to_vec()));
        if let Some(content_type) = content_type {
            request = request.content_type(content_type);
        }
        request.send().await.map_err(s3_error)?;
        Ok(())
    }

    /// 生成与实际 bucket 及键前缀一致的公开访问 URL。
    ///
    /// # 参数
    /// * `path` - 传入 `save_with_content_type` 的相对存储路径。
    ///
    /// # 返回
    /// 返回 `public_base_url` 与完整对象键拼接后的 URL。
    ///
    /// # 错误
    /// 对象路径无效时返回错误。
    pub fn public_url<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let key = self.object_key(path.as_ref())?;
        let mut url = self.public_base_url.clone();
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| Error::InvalidConfig("S3 public_base_url 不能作为分层 URL".to_string()))?;
        segments.pop_if_empty();
        for segment in key.split('/') {
            segments.push(segment);
        }
        drop(segments);
        Ok(url.to_string())
    }

    /// 读取 S3 对象的完整内容。
    ///
    /// 大对象改用 `read_stream` 边下边处理，避免全量常驻内存。
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
        let stream = self.read_stream(path).await?;
        let body = stream.collect().await.map_err(s3_error)?;
        Ok(body.into_bytes().to_vec())
    }

    /// 以流式读取 S3 对象，供大对象边下边处理。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    ///
    /// # 返回
    /// 返回可消费的字节流。
    ///
    /// # 错误
    /// 对象不存在时返回 `Error::NotFound`；路径无效或 S3 请求失败时返回错误。
    pub async fn read_stream<P: AsRef<Path>>(&self, path: P) -> Result<ByteStream> {
        let key = self.object_key(path.as_ref())?;
        let response =
            self.client.get_object().bucket(&self.bucket).key(key).send().await.map_err(get_error)?;
        Ok(response.body)
    }

    /// 为浏览器直传初始化 S3 分片上传。
    ///
    /// 大文件不再经过应用服务器内存：调用方把返回的分片地址直接交给浏览器，
    /// 浏览器逐片 PUT 到对象存储，最后由服务端合并。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    /// * `content_type` - 合并后对象的 HTTP Content-Type。
    ///
    /// # 返回
    /// 返回 S3 分片上传标识。
    ///
    /// # 错误
    /// 路径无效或 S3 `CreateMultipartUpload` 失败时返回错误。
    pub async fn create_multipart_upload<P: AsRef<Path>>(
        &self,
        path: P,
        content_type: Option<&str>,
    ) -> Result<String> {
        let key = self.object_key(path.as_ref())?;
        let mut request = self.client.create_multipart_upload().bucket(&self.bucket).key(key);
        if let Some(content_type) = content_type {
            request = request.content_type(content_type);
        }
        let response = request.send().await.map_err(s3_error)?;
        response
            .upload_id()
            .map(str::to_string)
            .ok_or_else(|| Error::S3("对象存储未返回分片上传标识".to_string()))
    }

    /// 为单个分片签发浏览器可直接 PUT 的预签名地址。
    ///
    /// 地址内已包含签名，浏览器请求时不得再附加应用鉴权头；
    /// 读取分片响应 `ETag` 头需要对象存储桶跨域配置暴露该头。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    /// * `upload_id` - 分片上传标识。
    /// * `part_number` - 分片序号（1 起）。
    /// * `expires_in` - 地址有效期。
    ///
    /// # 返回
    /// 返回预签名 PUT 地址。
    ///
    /// # 错误
    /// 路径无效、分片序号非法或签名失败时返回错误。
    pub async fn presign_upload_part<P: AsRef<Path>>(
        &self,
        path: P,
        upload_id: &str,
        part_number: i32,
        expires_in: Duration,
    ) -> Result<String> {
        if !(MIN_PART_NUMBER..=MAX_PART_NUMBER).contains(&part_number) {
            return Err(Error::S3("分片序号必须在 1-10000 之间".to_string()));
        }
        let key = self.object_key(path.as_ref())?;
        let config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|error| Error::S3(format!("预签名有效期非法: {error}")))?;
        let presigned = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            // 空体仅用于签名占位，不产生网络体。
            .body(ByteStream::from_static(b""))
            .presigned(config)
            .await
            .map_err(s3_error)?;
        Ok(presigned.uri().to_string())
    }

    /// 合并浏览器已直传的分片为完整对象。
    ///
    /// 调用方须按序号升序提供分片，本方法不做排序与去重，原序透传。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    /// * `upload_id` - 分片上传标识。
    /// * `parts` - 分片序号与 ETag 列表。
    ///
    /// # 返回
    /// 成功时返回空。
    ///
    /// # 错误
    /// 路径无效、分片列表为空、分片序号或 ETag 非法，或 S3 `CompleteMultipartUpload` 失败时返回错误。
    pub async fn complete_multipart_upload<P: AsRef<Path>>(
        &self,
        path: P,
        upload_id: &str,
        parts: Vec<UploadedPart>,
    ) -> Result<()> {
        validate_complete_parts(upload_id, &parts)?;
        let key = self.object_key(path.as_ref())?;
        let completed = parts
            .into_iter()
            .map(|part| {
                CompletedPart::builder()
                    .part_number(part.part_number)
                    .e_tag(part.etag.trim_matches('"'))
                    .build()
            })
            .collect::<Vec<_>>();
        let upload = CompletedMultipartUpload::builder().set_parts(Some(completed)).build();
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(upload)
            .send()
            .await
            .map_err(s3_error)?;
        Ok(())
    }

    /// 取消分片上传并清理对象存储侧已上传的分片。
    ///
    /// # 参数
    /// * `path` - 相对存储路径。
    /// * `upload_id` - 分片上传标识。
    ///
    /// # 错误
    /// 路径无效或 S3 `AbortMultipartUpload` 失败时返回错误。
    pub async fn abort_multipart_upload<P: AsRef<Path>>(&self, path: P, upload_id: &str) -> Result<()> {
        let key = self.object_key(path.as_ref())?;
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(s3_error)?;
        Ok(())
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

        self.client.delete_object().bucket(&self.bucket).key(key).send().await.map_err(s3_error)?;
        Ok(())
    }

    /// 返回加上可选前缀的规范 S3 对象键。
    fn object_key(&self, path: &Path) -> Result<String> {
        let path = object_key_path(path)?;
        Ok(self.key_prefix.as_ref().map(|prefix| format!("{prefix}/{path}")).unwrap_or(path))
    }

    /// 通过 `HeadObject` 区分对象不存在与存储服务失败。
    async fn object_exists(&self, key: &str) -> Result<bool> {
        match self.client.head_object().bucket(&self.bucket).key(key).send().await {
            Ok(_) => Ok(true),
            Err(error) if head_not_found(&error) => Ok(false),
            Err(error) => Err(s3_error(error)),
        }
    }

    #[cfg(test)]
    /// 用可控 AWS SDK 客户端构造测试实例。
    fn from_client(
        client: Client,
        bucket: &str,
        key_prefix: Option<&str>,
        public_base_url_value: &str,
    ) -> Result<Self> {
        Ok(Self {
            client,
            bucket: bucket.to_string(),
            key_prefix: normalize_prefix(key_prefix.map(str::to_owned))?,
            public_base_url: public_base_url(public_base_url_value)?,
        })
    }
}

/// 校验建立 S3 签名客户端必需的配置，并返回规范化结果。
///
/// # 参数
/// * `config` - 待校验的启动配置
///
/// # 返回
/// 返回规范化后的公开基础 URL 与对象键前缀。
///
/// # 错误
/// 配置非法时返回 `InvalidConfig`。
fn validate_config(config: &S3StorageConfig) -> Result<(Url, Option<String>)> {
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

    if config.endpoint.as_deref().is_some_and(|endpoint| !is_valid_endpoint(endpoint)) {
        return Err(Error::InvalidConfig("S3 endpoint 必须使用 http:// 或 https:// 绝对地址".to_string()));
    }
    if config.session_token.as_ref().is_some_and(|token| token.trim().is_empty() || token.trim() != token) {
        return Err(Error::InvalidConfig("S3 session_token 不能为空或包含首尾空白".to_string()));
    }

    let base_url = public_base_url(&config.public_base_url)?;
    let prefix = normalize_prefix(config.key_prefix.clone())?;

    Ok((base_url, prefix))
}

/// 校验合并分片的前置条件，与预签名同口径。
fn validate_complete_parts(upload_id: &str, parts: &[UploadedPart]) -> Result<()> {
    if upload_id.trim().is_empty() {
        return Err(Error::S3("分片上传标识不能为空".to_string()));
    }
    if parts.is_empty() {
        return Err(Error::S3("分片列表不能为空".to_string()));
    }
    for part in parts {
        if !(MIN_PART_NUMBER..=MAX_PART_NUMBER).contains(&part.part_number) {
            return Err(Error::S3("分片序号必须在 1-10000 之间".to_string()));
        }
        if part.etag.trim_matches('"').trim().is_empty() {
            return Err(Error::S3("分片 ETag 不能为空".to_string()));
        }
    }
    Ok(())
}

/// 解析并规范公开访问基础 URL，禁止查询与 fragment 污染对象 URL。
fn public_base_url(value: &str) -> Result<Url> {
    let mut url = Url::parse(value)
        .map_err(|_| Error::InvalidConfig("S3 public_base_url 必须是合法 URL".to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidConfig(
            "S3 public_base_url 必须是不含 query/fragment 的 HTTP(S) URL".to_string(),
        ));
    }
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
    Ok(url)
}

/// 判断 endpoint 是否为带主机部分的 HTTP(S) 绝对地址，与公开 URL 同口径。
fn is_valid_endpoint(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|host| !host.is_empty())
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
        || prefix.split('/').any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::InvalidConfig("S3 key_prefix 必须是不带首尾分隔符的相对对象键前缀".to_string()));
    }
    Ok(Some(prefix))
}

/// 将 S3 SDK 错误统一转换为存储错误。
fn s3_error(error: impl std::fmt::Display) -> Error {
    Error::S3(error.to_string())
}

/// 将 `GetObject` 的不存在语义映射到统一存储错误。
fn get_error<R>(error: SdkError<GetObjectError, R>) -> Error {
    if error.as_service_error().is_some_and(GetObjectError::is_no_such_key) {
        return Error::NotFound;
    }
    s3_error(error)
}

/// 判断 `HeadObject` 失败是否表示对象不存在。
fn head_not_found<R>(error: &SdkError<HeadObjectError, R>) -> bool {
    error.as_service_error().is_some_and(HeadObjectError::is_not_found)
}

#[cfg(test)]
mod tests {
    use aws_sdk_s3::Config;
    use aws_sdk_s3::config::{Credentials, Region};
    use aws_smithy_http_client::test_util::{CaptureRequestReceiver, capture_request};

    use super::*;

    /// 构造带固定测试端点的存储与请求捕获器，各用例只传差异参数。
    fn test_storage(
        bucket: &str,
        key_prefix: Option<&str>,
        public_base_url: &str,
    ) -> (S3Storage, CaptureRequestReceiver) {
        let (http_client, receiver) = capture_request(None);
        let sdk_config = Config::builder()
            .behavior_version_latest()
            .credentials_provider(test_credentials())
            .region(Region::new("us-east-1"))
            .endpoint_url("https://s3.example.com")
            .force_path_style(true)
            .http_client(http_client)
            .build();
        let storage =
            S3Storage::from_client(Client::from_conf(sdk_config), bucket, key_prefix, public_base_url)
                .expect("测试存储必须构造成功");
        (storage, receiver)
    }

    /// S3 保存必须使用 bucket、键前缀和跨平台对象键发出 `PutObject`。
    #[tokio::test]
    async fn saves_object_with_configured_prefix() -> Result<()> {
        let (storage, request_receiver) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

        storage.save_with_content_type("images/example.png", b"image-bytes", Some("image/png")).await?;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "PUT");
        assert_eq!(
            request.uri(),
            "https://s3.example.com/erp-assets/tenant-a/uploads/images/example.png?x-id=PutObject"
        );
        assert_eq!(request.body().bytes(), Some(b"image-bytes".as_slice()));
        assert_eq!(request.headers().get("content-type").unwrap(), "image/png");
        Ok(())
    }

    /// S3 读取必须从同一 bucket 和键前缀发出 `GetObject`。
    #[tokio::test]
    async fn reads_object_with_configured_prefix() -> Result<()> {
        let (storage, request_receiver) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

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

    /// 公开 URL 必须包含基础路径、键前缀与经编码的对象路径。
    #[test]
    fn builds_public_url_from_complete_object_key() -> Result<()> {
        let (storage, _) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets/");

        let url = storage.public_url("images/中 文.png")?;

        assert_eq!(url, "https://cdn.example.com/assets/tenant-a/uploads/images/%E4%B8%AD%20%E6%96%87.png");
        Ok(())
    }

    /// S3 对象键不得使用父目录分量越过配置前缀。
    #[tokio::test]
    async fn rejects_parent_directory_object_key() -> Result<()> {
        let (storage, _) = test_storage("erp-assets", None, "https://cdn.example.com");

        let result = storage.save_with_content_type("../escaped.txt", b"escaped", None).await;

        assert!(matches!(result, Err(Error::PathError(_))));
        Ok(())
    }

    /// S3 启动配置必须提供非空 bucket、region 和签名凭证。
    #[test]
    fn rejects_incomplete_s3_config() {
        let result = S3Storage::new(S3StorageConfig::new(
            "",
            "us-east-1",
            "access-key",
            "secret-key",
            "https://cdn.example.com",
        ));

        assert!(matches!(result, Err(Error::InvalidConfig(_))));
    }

    /// S3 启动构造器默认关闭可选端点与前缀。
    #[test]
    fn storage_config_constructor_sets_required_fields() {
        let config = S3StorageConfig::new(
            "erp-assets",
            "us-east-1",
            "access-key",
            "secret-key",
            "https://cdn.example.com",
        )
        .with_endpoint("https://s3.example.com")
        .with_key_prefix("tenant-a/uploads")
        .with_force_path_style(true);
        assert_eq!(config.bucket, "erp-assets");
        assert_eq!(config.endpoint.as_deref(), Some("https://s3.example.com"));
        assert_eq!(config.key_prefix.as_deref(), Some("tenant-a/uploads"));
        assert!(config.force_path_style);
        assert!(config.session_token.is_none());
    }

    /// 分片上传初始化必须向同一 bucket 和键前缀发出 `CreateMultipartUpload`。
    #[tokio::test]
    async fn creates_multipart_upload_with_configured_prefix() -> Result<()> {
        let (storage, request_receiver) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

        let _ = storage.create_multipart_upload("imports/req-1.xlsx", Some("application/octet-stream")).await;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "POST");
        assert!(request.uri().contains("tenant-a/uploads/imports/req-1.xlsx"));
        assert!(request.uri().contains("uploads"));
        Ok(())
    }

    /// 合并分片必须发出 `CompleteMultipartUpload` 并去除 ETag 引号。
    #[tokio::test]
    async fn completes_multipart_upload_without_etag_quotes() -> Result<()> {
        let (storage, request_receiver) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

        // 空 200 模拟无法解析合并响应 XML，仅断言请求体；成功语义由校验测试覆盖。
        storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "upload-id",
                vec![super::UploadedPart { part_number: 1, etag: "\"etag-1\"".to_string() }],
            )
            .await
            .ok();

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "POST");
        let body = request
            .body()
            .bytes()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        assert!(body.contains("<ETag>etag-1</ETag>"));
        assert!(!body.contains("&quot;"));
        Ok(())
    }

    /// 取消分片上传必须发出 `AbortMultipartUpload`。
    #[tokio::test]
    async fn aborts_multipart_upload() -> Result<()> {
        let (storage, request_receiver) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

        storage.abort_multipart_upload("imports/req-1.xlsx", "upload-id").await?;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "DELETE");
        assert!(request.uri().contains("tenant-a/uploads/imports/req-1.xlsx"));
        Ok(())
    }

    /// 分片预签名地址必须携带上传标识与分片序号且无需网络请求。
    #[tokio::test]
    async fn presigns_upload_part_without_network() -> Result<()> {
        let (storage, _) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

        let url = storage
            .presign_upload_part("imports/req-1.xlsx", "upload-id", 2, std::time::Duration::from_secs(7200))
            .await?;

        assert!(url.contains("partNumber=2"));
        assert!(url.contains("uploadId=upload-id"));
        Ok(())
    }

    /// 空分片列表合并必须在请求发出前失败。
    #[tokio::test]
    async fn rejects_complete_with_empty_parts() -> Result<()> {
        let (storage, _) = test_storage("erp-assets", None, "https://cdn.example.com");
        let result = storage.complete_multipart_upload("imports/req-1.xlsx", "upload-id", vec![]).await;

        assert!(matches!(result, Err(Error::S3(_))));
        Ok(())
    }

    /// 非法分片序号预签名必须在请求发出前失败。
    #[tokio::test]
    async fn rejects_presign_with_invalid_part_number() -> Result<()> {
        let (storage, _) = test_storage("erp-assets", None, "https://cdn.example.com");
        let result = storage
            .presign_upload_part("imports/req-1.xlsx", "upload-id", 0, std::time::Duration::from_secs(60))
            .await;

        assert!(matches!(result, Err(Error::S3(_))));
        Ok(())
    }

    /// 合并分片必须校验上传标识、序号范围与 ETag 非空。
    #[tokio::test]
    async fn rejects_complete_with_invalid_upload_id_or_parts() -> Result<()> {
        let (storage, request_receiver) = test_storage("erp-assets", None, "https://cdn.example.com");

        let result = storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "  ",
                vec![super::UploadedPart { part_number: 1, etag: "etag-1".to_string() }],
            )
            .await;
        assert!(matches!(result, Err(Error::S3(_))));

        let result = storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "upload-id",
                vec![super::UploadedPart { part_number: 0, etag: "etag-1".to_string() }],
            )
            .await;
        assert!(matches!(result, Err(Error::S3(_))));

        let result = storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "upload-id",
                vec![super::UploadedPart { part_number: 1, etag: "  ".to_string() }],
            )
            .await;
        assert!(matches!(result, Err(Error::S3(_))));

        request_receiver.expect_no_request();
        Ok(())
    }

    /// 返回仅用于捕获 SDK 请求的固定测试凭证。
    fn test_credentials() -> Credentials {
        Credentials::new("test-access-key", "test-secret-key", None, None, "storage-test")
    }
}
