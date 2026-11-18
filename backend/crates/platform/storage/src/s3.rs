use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::{Client, Config};
use url::Url;

use crate::content_cache::ContentCache;
use crate::cos::original_image_uri;
use crate::path::{is_blank_or_padded, normalize_prefix, object_key_path};
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
    /// 用 bucket、签名 region、访问密钥和公开访问基址组装基本配置，供应用启动时创建共享客户端。
    /// 默认使用 SDK 的端点与寻址方式，不设 session token 或对象键前缀；连接参数由部署按需补充。
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

    /// 连接 MinIO 等 S3 兼容服务时指定其 HTTP(S) API 端点；对外访问地址仍由 `public_base_url` 决定。
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// 在同一 bucket 内隔离不同部署的文件时设置统一的对象键前缀，读写和公开 URL 都将使用它。
    pub fn with_key_prefix(mut self, key_prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(key_prefix.into());
        self
    }

    /// 兼容服务要求在路径中而非子域名中传 bucket 时启用 path-style 寻址。
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
    content_cache: Arc<Mutex<ContentCache>>,
}

/// 已直传分片的序号与 ETag，用于合并分片上传。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedPart {
    /// 分片序号（1 起）。
    pub part_number: i32,
    /// 分片 PUT 响应 `ETag` 头（可带引号，合并时自动去除）。
    pub etag: String,
}

impl UploadedPart {
    // 校验和提交分片列表共用去引号后的 ETag，兼容浏览器从分片 PUT 响应头读到的带引号值。
    fn normalized_etag(&self) -> &str {
        self.etag.trim_matches('"')
    }

    // 合并前拒绝超出 1～10000 的分片号和去引号后为空白的 ETag，避免把无效分片清单发给 S3。
    fn validate(&self) -> Result<()> {
        validate_part_number(self.part_number)?;
        if self.normalized_etag().trim().is_empty() {
            return Err(Error::S3("分片 ETag 不能为空".to_string()));
        }
        Ok(())
    }
}

impl S3Storage {
    /// 应用启动时用部署配置创建 S3 客户端并复用，避免为每次请求重复建立客户端和连接池。
    ///
    /// # Errors
    /// 必填的 bucket、region、访问密钥或已提供的 session token 为空或含首尾空白时，返回
    /// [`Error::InvalidConfig`]。自定义端点须是带主机的 HTTP(S) 绝对地址，公开基址还不得带查询或片段。
    /// 对象键前缀不得为空白、包含首尾空白或斜杠、反斜杠、空段、`.` 或 `..`。
    pub fn new(config: S3StorageConfig) -> Result<Self> {
        let public_base_url = validate_config(&config)?;
        let key_prefix = normalize_prefix(config.key_prefix)?;

        let credentials = Credentials::new(
            config.access_key_id,
            config.secret_access_key,
            config.session_token,
            None,
            "erp-config",
        );
        let mut sdk_config = Config::builder()
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
            content_cache: Arc::new(Mutex::new(ContentCache::default())),
        })
    }

    /// 调用方只有借用的缓冲区时，将小文件及可选的已校验 MIME 类型写入相对对象路径。
    /// 本入口会复制整个缓冲区；持有 `Vec<u8>` 时用 [`Self::save_owned_with_content_type`]，
    /// 大文件走 [`Self::create_multipart_upload`] 开始的直传流程。错误与缓存失效规则同持有所有权的入口。
    pub async fn save_with_content_type<P: AsRef<Path>>(
        &self,
        path: P,
        content: &[u8],
        content_type: Option<&str>,
    ) -> Result<()> {
        self.save_owned_with_content_type(path, content.to_vec(), content_type).await
    }

    /// 将已缓冲文件的 `Vec<u8>` 转给 SDK 构造上传请求，避免再复制整份文件。
    /// 调用前应先完成类型校验、大小记录和指纹计算。对象键校验通过后，发起 PUT 前后均尝试使同键缓存失效；
    /// 即使 PUT 失败也不保留旧缓存，缓存锁中毒时跳过失效。取消调用时遵循 SDK 上传语义。
    ///
    /// # Errors
    /// 相对路径非法时返回 [`Error::PathError`]，不影响缓存；上传失败返回 [`Error::S3`]。
    pub async fn save_owned_with_content_type<P: AsRef<Path>>(
        &self,
        path: P,
        content: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<()> {
        let key = self.object_key(path.as_ref())?;
        self.invalidate_content(&key);

        let started = Instant::now();
        let mut request =
            self.client.put_object().bucket(&self.bucket).key(&key).body(ByteStream::from(content));
        if let Some(content_type) = content_type {
            request = request.content_type(content_type);
        }
        let result = request.send().await.map_err(s3_error);

        self.invalidate_content(&key);
        record_operation("put_object", started, result.is_ok());
        result?;
        Ok(())
    }

    /// 应用可在启动时限时发送 `HeadBucket`，为共享客户端预建连接，不读写任何对象。
    /// 请求失败返回 [`Error::S3`]；即使被拒绝也可能已建立可复用的 TLS 连接，预热结果不应决定启动资格。
    pub async fn warm_connection(&self) -> Result<()> {
        let started = Instant::now();
        let result = self.client.head_bucket().bucket(&self.bucket).send().await.map_err(s3_error);
        record_operation("head_bucket_warmup", started, result.is_ok());
        result?;
        Ok(())
    }

    /// 为需要公开分发的对象生成 CDN 或 bucket URL，按路径段编码并保留公开基址和配置的键前缀。
    /// 传入与存储读写相同的相对路径，无需自行拼接或编码；不检查对象是否存在，也不改变访问权限。
    ///
    /// # Errors
    /// 对象路径非法时返回 [`Error::PathError`]；基址不可用于追加路径段时返回 [`Error::InvalidConfig`]。
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

    /// 将 S3 对象完整读入内存，不使用不可变字节缓存；适用于小对象，尤其是可能更新的内容。
    /// 处理大对象时改用 [`Self::read_stream`]，避免整份数据常驻内存。
    ///
    /// # Errors
    /// 路径非法返回 [`Error::PathError`]，对象不存在返回 [`Error::NotFound`]，其余 GET 或正文读取失败返回 [`Error::S3`]。
    pub async fn read<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>> {
        let key = self.object_key(path.as_ref())?;
        self.read_key(key).await
    }

    async fn read_key(&self, key: String) -> Result<Vec<u8>> {
        let stream = self.read_key_stream(key).await?;

        let started = Instant::now();
        let body = stream.collect().await.map_err(s3_error);
        record_operation("get_object_body", started, body.is_ok());
        let body = body?;
        Ok(body.into_bytes().to_vec())
    }

    /// 按相对对象路径和已登记指纹复用不可变对象的完整字节，减少对重复下载的 GET 请求。
    /// 仅用于新键保存、内容不可覆盖的受控对象；每次调用仍须做当前业务授权、文件治理校验及敏感读取审计，
    /// 交付前重验权限、文件状态和版本。缓存不保存授权、审计结果或 HTTP 响应。
    ///
    /// 只缓存成功 GET 的正文，绝对期限 30 秒，单对象最多 512 KiB、总量 8 MiB、最多 256 项。
    /// 指纹变化或经本客户端写入、删除时失效；外部或跨进程同键变更可能在期限内不可见，也不保证同键读写线性化。
    /// 可变对象继续用 [`Self::read`]；缓存锁中毒只退回直接读取，路径及读取错误与该方法相同。
    pub async fn read_immutable<P: AsRef<Path>>(&self, path: P, fingerprint: &str) -> Result<Vec<u8>> {
        let key = self.object_key(path.as_ref())?;
        let generation = self.content_cache.lock().ok().and_then(|cache| cache.generation());
        if !fingerprint.is_empty()
            && let Some(bytes) = self
                .content_cache
                .lock()
                .ok()
                .and_then(|mut cache| cache.read(&key, fingerprint, Instant::now()))
        {
            tracing::info!(operation = "immutable_content", cache_hit = true, "storage content read");
            return Ok(bytes);
        }

        let bytes = self.read_key(key.clone()).await?;
        // 缓存锁中毒不改变原对象读取的成功结果。
        if let Ok(mut cache) = self.content_cache.lock() {
            cache.insert_if_current(generation, key, fingerprint.to_string(), &bytes, Instant::now());
        }
        tracing::info!(operation = "immutable_content", cache_hit = false, "storage content read");
        Ok(bytes)
    }

    /// 为大文件下载或逐块处理打开 S3 字节流，避免先把完整对象读入内存。
    ///
    /// # Errors
    /// 路径非法返回 [`Error::PathError`]，对象不存在返回 [`Error::NotFound`]，其他 GET 失败返回 [`Error::S3`]。
    pub async fn read_stream<P: AsRef<Path>>(&self, path: P) -> Result<ByteStream> {
        let key = self.object_key(path.as_ref())?;
        self.read_key_stream(key).await
    }

    async fn read_key_stream(&self, key: String) -> Result<ByteStream> {
        let started = Instant::now();
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .customize()
            .map_request(|mut request| {
                if let Some(uri) = original_image_uri(request.uri())? {
                    request.set_uri(uri).map_err(s3_error)?;
                }
                Ok::<_, Error>(request)
            })
            .send()
            .await
            .map_err(get_error);
        record_operation("get_object_headers", started, response.is_ok());
        let response = response?;
        Ok(response.body)
    }

    /// 在浏览器开始直传大文件前，为相对对象路径创建上传会话，可指定最终对象的 MIME 类型。
    /// 返回的上传 ID 用来签发分片 PUT 地址、完成上传或取消；文件字节由浏览器直接传给对象存储。
    ///
    /// # Errors
    /// 路径非法返回 [`Error::PathError`]；初始化请求失败或响应缺少上传 ID 时返回 [`Error::S3`]。
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

    /// 上传会话建立后，为指定分片签发有时限的 PUT 地址；浏览器应直接上传，不附加应用鉴权头。
    /// 对象存储桶的跨域策略须暴露响应 `ETag`，浏览器才能将分片凭据交回服务端用于合并。
    ///
    /// # Errors
    /// 路径非法返回 [`Error::PathError`]；序号不在 1～10000、有效期不被 SDK 接受或签名失败时返回 [`Error::S3`]。
    pub async fn presign_upload_part<P: AsRef<Path>>(
        &self,
        path: P,
        upload_id: &str,
        part_number: i32,
        expires_in: Duration,
    ) -> Result<String> {
        validate_part_number(part_number)?;
        let key = self.object_key(path.as_ref())?;
        let config = PresigningConfig::expires_in(expires_in)
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

    /// 浏览器上传全部分片后，使用上传 ID 和各分片的 ETag 将它们合并为最终对象。
    /// 调用方须按序号升序提供分片；此处去除 ETag 引号，不排序也不去重。
    /// 请求发送前后均尝试使同键缓存失效，不因合并失败保留旧缓存；缓存锁中毒时跳过失效。
    ///
    /// # Errors
    /// 空白上传 ID、空分片列表、序号不在 1～10000 或去引号后为空白的 ETag 返回 [`Error::S3`]，缓存不变。
    /// 路径非法返回 [`Error::PathError`]，缓存不变；合并请求失败也返回 [`Error::S3`]。
    pub async fn complete_multipart_upload<P: AsRef<Path>>(
        &self,
        path: P,
        upload_id: &str,
        parts: Vec<UploadedPart>,
    ) -> Result<()> {
        validate_complete_parts(upload_id, &parts)?;
        let key = self.object_key(path.as_ref())?;
        self.invalidate_content(&key);

        let started = Instant::now();
        let completed = parts
            .into_iter()
            .map(|part| {
                CompletedPart::builder().part_number(part.part_number).e_tag(part.normalized_etag()).build()
            })
            .collect::<Vec<_>>();
        let upload = CompletedMultipartUpload::builder().set_parts(Some(completed)).build();

        let result = self
            .client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .multipart_upload(upload)
            .send()
            .await
            .map_err(s3_error);

        self.invalidate_content(&key);
        record_operation("complete_multipart_upload", started, result.is_ok());
        result?;
        Ok(())
    }

    /// 直传流程放弃或无法继续时按相对路径和上传 ID 取消会话，清理存储侧已上传但未合并的分片。
    /// 路径非法返回 [`Error::PathError`]，取消请求失败返回 [`Error::S3`]。
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

    /// 在业务流程确定不再保留文件后，按相对路径删除存储对象，先用 `HeadObject` 确认对象存在。
    /// 对象键校验通过便尝试使同键缓存失效，不因后续检查或删除失败保留旧缓存；缓存锁中毒时跳过失效。
    ///
    /// # Errors
    /// 路径非法返回 [`Error::PathError`]，缓存不变；对象已不存在返回 [`Error::NotFound`]，检查或删除失败返回 [`Error::S3`]。
    pub async fn delete<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let key = self.object_key(path.as_ref())?;
        self.invalidate_content(&key);

        match self.client.head_object().bucket(&self.bucket).key(&key).send().await {
            Ok(_) => {},
            Err(error) if error.as_service_error().is_some_and(HeadObjectError::is_not_found) => {
                return Err(Error::NotFound);
            },
            Err(error) => return Err(s3_error(error)),
        }

        let result =
            self.client.delete_object().bucket(&self.bucket).key(&key).send().await.map_err(s3_error);

        self.invalidate_content(&key);
        result?;
        Ok(())
    }

    // 对象变更前后同步推进缓存代次并移除同键字节，不跨网络 await 持锁；锁中毒时跳过缓存操作。
    fn invalidate_content(&self, key: &str) {
        if let Ok(mut cache) = self.content_cache.lock() {
            cache.invalidate(key);
        }
    }

    // 读写或构造公开 URL 前统一校验相对路径并加上配置前缀，确保指向同一个 S3 对象。
    fn object_key(&self, path: &Path) -> Result<String> {
        let path = object_key_path(path)?;
        Ok(self.key_prefix.as_ref().map(|prefix| format!("{prefix}/{path}")).unwrap_or(path))
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
            content_cache: Arc::new(Mutex::new(ContentCache::default())),
        })
    }
}

// S3 请求或正文读取结束后记录该阶段的耗时和成功状态，不记录内容、对象键、签名或凭证。
fn record_operation(operation: &str, started: Instant, success: bool) {
    tracing::info!(
        operation,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        success,
        "storage operation completed"
    );
}

// 创建 SDK 客户端前检查 bucket、签名参数、显式端点和公开基址，避免将配置错误拖到首次请求才发现。
// 解析出的公开基址供后续拼接对象 URL；配置不合法返回 InvalidConfig。
fn validate_config(config: &S3StorageConfig) -> Result<Url> {
    for (name, value) in [
        ("bucket", config.bucket.as_str()),
        ("region", config.region.as_str()),
        ("access_key_id", config.access_key_id.as_str()),
        ("secret_access_key", config.secret_access_key.as_str()),
    ] {
        if is_blank_or_padded(value) {
            return Err(Error::InvalidConfig(format!("S3 {name} 不能为空或包含首尾空白")));
        }
    }

    if config.endpoint.as_deref().is_some_and(|endpoint| !is_valid_endpoint(endpoint)) {
        return Err(Error::InvalidConfig("S3 endpoint 必须使用 http:// 或 https:// 绝对地址".to_string()));
    }
    if config.session_token.as_ref().is_some_and(|token| is_blank_or_padded(token)) {
        return Err(Error::InvalidConfig("S3 session_token 不能为空或包含首尾空白".to_string()));
    }

    public_base_url(&config.public_base_url)
}

// 发合并请求前检查上传 ID 及每个分片的序号和 ETag，拒绝空清单；不负责排序或去重。
fn validate_complete_parts(upload_id: &str, parts: &[UploadedPart]) -> Result<()> {
    if upload_id.trim().is_empty() {
        return Err(Error::S3("分片上传标识不能为空".to_string()));
    }
    if parts.is_empty() {
        return Err(Error::S3("分片列表不能为空".to_string()));
    }
    for part in parts {
        part.validate()?;
    }
    Ok(())
}

// 预签名和合并使用同一 S3 分片号范围（1～10000），避免签出无法合并的分片。
fn validate_part_number(part_number: i32) -> Result<()> {
    if !(MIN_PART_NUMBER..=MAX_PART_NUMBER).contains(&part_number) {
        return Err(Error::S3("分片序号必须在 1-10000 之间".to_string()));
    }
    Ok(())
}

// 自定义 API 端点和公开基址共用协议检查；主机及查询参数等限制由各自入口判断。
fn is_http_scheme(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

// 在装配客户端时解析公开基址，去掉路径末尾斜杠供对象键拼接；拒绝带查询或片段的基址，
// 避免把基址参数或锚点错误地传给每个公开对象 URL。
fn public_base_url(value: &str) -> Result<Url> {
    let mut url = Url::parse(value)
        .map_err(|_| Error::InvalidConfig("S3 public_base_url 必须是合法 URL".to_string()))?;
    if !is_http_scheme(&url) || url.host_str().is_none() || url.query().is_some() || url.fragment().is_some()
    {
        return Err(Error::InvalidConfig(
            "S3 public_base_url 必须是不含 query/fragment 的 HTTP(S) URL".to_string(),
        ));
    }

    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
    Ok(url)
}

// 创建 SDK 客户端前确认自定义端点有可连接的主机以及 HTTP(S) 协议；不套用公开基址额外的限制。
fn is_valid_endpoint(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint) else {
        return false;
    };
    is_http_scheme(&url) && url.host_str().is_some_and(|host| !host.is_empty())
}

// 将 SDK 请求和正文读取的通用失败归到存储错误，调用方无需依赖底层 SDK 错误类型。
fn s3_error(error: impl std::fmt::Display) -> Error {
    Error::S3(error.to_string())
}

// 读取对象时单独识别 GetObject 的 NoSuchKey，让业务方区分对象缺失和其他 S3 故障。
fn get_error<R>(error: SdkError<GetObjectError, R>) -> Error {
    if error.as_service_error().is_some_and(GetObjectError::is_no_such_key) {
        return Error::NotFound;
    }
    s3_error(error)
}

#[cfg(test)]
mod tests {
    use aws_smithy_http_client::test_util::{CaptureRequestReceiver, capture_request};

    use super::*;

    const TEST_BUCKET: &str = "erp-assets";
    const TEST_PREFIX: &str = "tenant-a/uploads";
    const TEST_PUBLIC_BASE_URL: &str = "https://cdn.example.com/assets";

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

    // 默认带键前缀的测试存储，覆盖多数用例的同一 bucket 与 CDN。
    fn test_storage_with_prefix() -> (S3Storage, CaptureRequestReceiver) {
        test_storage(TEST_BUCKET, Some(TEST_PREFIX), TEST_PUBLIC_BASE_URL)
    }

    // 不带键前缀的测试存储，供路径与分片校验用例使用。
    fn test_storage_without_prefix() -> (S3Storage, CaptureRequestReceiver) {
        test_storage(TEST_BUCKET, None, "https://cdn.example.com")
    }

    // 捕获真实 SDK 生成的 PUT，验证 bucket 和前缀正确，并确保上传正文与已校验 MIME 原样送出。
    #[tokio::test]
    async fn saves_object_with_configured_prefix() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();

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

    // 以捕获的 GET 确认读取沿用保存时的 bucket 和键前缀，不会查到另一个对象。
    #[tokio::test]
    async fn reads_object_with_configured_prefix() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();

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

    // 验证真实 SDK 生成的 COS GET 同时带授权头和原图参数，且保留对象路径的百分号编码。
    #[tokio::test]
    async fn reads_cos_original_image_with_signed_query() -> Result<()> {
        let (http_client, receiver) = capture_request(None);
        let sdk_config = Config::builder()
            .behavior_version_latest()
            .credentials_provider(test_credentials())
            .region(Region::new("ap-guangzhou"))
            .endpoint_url("https://cos.ap-guangzhou.myqcloud.com")
            .force_path_style(true)
            .http_client(http_client)
            .build();
        let storage = S3Storage::from_client(
            Client::from_conf(sdk_config),
            TEST_BUCKET,
            Some(TEST_PREFIX),
            TEST_PUBLIC_BASE_URL,
        )?;

        storage.read_immutable("receipts/bank copy.png", "registered-fingerprint").await?;

        let request = receiver.expect_request();
        assert_eq!(request.method(), "GET");
        assert_eq!(
            request.uri(),
            "https://cos.ap-guangzhou.myqcloud.com/erp-assets/tenant-a/uploads/receipts/bank%20copy.png?x-id=GetObject&ci-process=originImage"
        );
        assert!(request.headers().get("authorization").is_some());
        Ok(())
    }

    // 先确认成功读取发出 GET，再用同一指纹重读，并检查缓存入口仍拒绝含父目录的路径。
    #[tokio::test]
    async fn immutable_content_reuses_only_a_successful_get() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();
        assert_eq!(storage.read_immutable("images/private.png", "current-hmac").await?, Vec::<u8>::new());
        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "GET");
        assert_eq!(storage.read_immutable("images/private.png", "current-hmac").await?, Vec::<u8>::new());

        let rejected = storage.read_immutable("../images/private.png", "current-hmac").await;
        assert!(matches!(rejected, Err(Error::PathError(_))));
        Ok(())
    }

    // 预填同键旧字节，验证转移所有权的上传发送新正文，并在请求结束后不再命中旧缓存。
    #[tokio::test]
    async fn owned_upload_invalidates_previous_content() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();
        let key = storage.object_key(Path::new("images/private.png"))?;
        storage.content_cache.lock().unwrap().insert(key.clone(), "old".into(), b"old", Instant::now());

        storage
            .save_owned_with_content_type("images/private.png", b"new".to_vec(), Some("image/png"))
            .await?;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "PUT");
        assert_eq!(request.body().bytes(), Some(b"new".as_slice()));
        assert_eq!(storage.content_cache.lock().unwrap().read(&key, "old", Instant::now()), None);
        Ok(())
    }

    // 验证公开 CDN 地址保留基址路径和对象键前缀，并按路径段编码中文和空格。
    #[test]
    fn builds_public_url_from_complete_object_key() -> Result<()> {
        let (storage, _) = test_storage(TEST_BUCKET, Some(TEST_PREFIX), "https://cdn.example.com/assets/");

        let url = storage.public_url("images/中 文.png")?;
        assert_eq!(url, "https://cdn.example.com/assets/tenant-a/uploads/images/%E4%B8%AD%20%E6%96%87.png");
        Ok(())
    }

    // 验证上传入口拒绝含父目录的路径，防止绕过对象键的相对路径约定。
    #[tokio::test]
    async fn rejects_parent_directory_object_key() -> Result<()> {
        let (storage, _) = test_storage_without_prefix();
        let result = storage.save_with_content_type("../escaped.txt", b"escaped", None).await;
        assert!(matches!(result, Err(Error::PathError(_))));
        Ok(())
    }

    // 用空 bucket 验证启动时即拒绝不完整配置，不把错误留到首个 S3 请求。
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

    // 确认基本配置可叠加兼容端点、键前缀和 path-style 设置，且未提供的 session token 保持空缺。
    #[test]
    fn storage_config_constructor_sets_required_fields() {
        let config = S3StorageConfig::new(
            TEST_BUCKET,
            "us-east-1",
            "access-key",
            "secret-key",
            "https://cdn.example.com",
        )
        .with_endpoint("https://s3.example.com")
        .with_key_prefix(TEST_PREFIX)
        .with_force_path_style(true);

        assert_eq!(config.bucket, TEST_BUCKET);
        assert_eq!(config.endpoint.as_deref(), Some("https://s3.example.com"));
        assert_eq!(config.key_prefix.as_deref(), Some(TEST_PREFIX));
        assert!(config.force_path_style);
        assert!(config.session_token.is_none());
    }

    // 捕获分片初始化的 POST，确认直传会话与普通上传使用相同的对象键前缀。
    #[tokio::test]
    async fn creates_multipart_upload_with_configured_prefix() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();

        let _ = storage.create_multipart_upload("imports/req-1.xlsx", Some("application/octet-stream")).await;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "POST");
        assert!(request.uri().contains("tenant-a/uploads/imports/req-1.xlsx"));
        assert!(request.uri().contains("uploads"));
        Ok(())
    }

    // 捕获合并分片的 POST 正文，确认浏览器返回的带引号 ETag 不会以引号写入合并 XML。
    #[tokio::test]
    async fn completes_multipart_upload_without_etag_quotes() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();

        // 空 200 模拟无法解析合并响应 XML，仅断言请求体；成功语义由校验测试覆盖。
        storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "upload-id",
                vec![UploadedPart { part_number: 1, etag: "\"etag-1\"".to_string() }],
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

    // 捕获取消会话的 DELETE，确认清理的是带配置前缀的目标对象所对应的分片。
    #[tokio::test]
    async fn aborts_multipart_upload() -> Result<()> {
        let (storage, request_receiver) = test_storage_with_prefix();

        storage.abort_multipart_upload("imports/req-1.xlsx", "upload-id").await?;

        let request = request_receiver.expect_request();
        assert_eq!(request.method(), "DELETE");
        assert!(request.uri().contains("tenant-a/uploads/imports/req-1.xlsx"));
        Ok(())
    }

    // 在没有真实网络的 SDK 客户端上验证预签名地址绑定上传 ID 和分片号。
    #[tokio::test]
    async fn presigns_upload_part_without_network() -> Result<()> {
        let (storage, _) =
            test_storage("erp-assets", Some("tenant-a/uploads"), "https://cdn.example.com/assets");

        let url = storage
            .presign_upload_part("imports/req-1.xlsx", "upload-id", 2, Duration::from_secs(7200))
            .await?;

        assert!(url.contains("partNumber=2"));
        assert!(url.contains("uploadId=upload-id"));
        Ok(())
    }

    // 验证没有任何分片可提交的会话会在合并前被拒绝。
    #[tokio::test]
    async fn rejects_complete_with_empty_parts() -> Result<()> {
        let (storage, _) = test_storage_without_prefix();
        let result = storage.complete_multipart_upload("imports/req-1.xlsx", "upload-id", vec![]).await;
        assert!(matches!(result, Err(Error::S3(_))));
        Ok(())
    }

    // 用序号 0 验证预签名不会为超出 S3 序号范围的分片生成 PUT 地址。
    #[tokio::test]
    async fn rejects_presign_with_invalid_part_number() -> Result<()> {
        let (storage, _) = test_storage_without_prefix();
        let result =
            storage.presign_upload_part("imports/req-1.xlsx", "upload-id", 0, Duration::from_secs(60)).await;
        assert!(matches!(result, Err(Error::S3(_))));
        Ok(())
    }

    // 分别传入空白上传 ID、非法序号和空白 ETag，确认合并在本地拒绝，SDK 不发送任何请求。
    #[tokio::test]
    async fn rejects_complete_with_invalid_upload_id_or_parts() -> Result<()> {
        let (storage, request_receiver) = test_storage_without_prefix();

        let result = storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "  ",
                vec![UploadedPart { part_number: 1, etag: "etag-1".to_string() }],
            )
            .await;
        assert!(matches!(result, Err(Error::S3(_))));

        let result = storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "upload-id",
                vec![UploadedPart { part_number: 0, etag: "etag-1".to_string() }],
            )
            .await;
        assert!(matches!(result, Err(Error::S3(_))));

        let result = storage
            .complete_multipart_upload(
                "imports/req-1.xlsx",
                "upload-id",
                vec![UploadedPart { part_number: 1, etag: "  ".to_string() }],
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
