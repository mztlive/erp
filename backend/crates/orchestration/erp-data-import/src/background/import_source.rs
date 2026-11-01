//! 切片间复用不可变输入；仅缓存成功加载的来源，不缓存任务状态或授权结果。

use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;

use application_core::command::CommandFingerprint;
use erp_support::BackgroundJob;
use tokio::sync::Mutex;

use super::import_budget::IMPORT_JOBS_PER_POLL;
use crate::Result;

/// 每个流程最多保留一轮任务；输入字节预算不等同于解析对象的堆内存上限。
pub(crate) struct ImportSourceCache<T> {
    entries: Mutex<VecDeque<Entry<T>>>,
    max_bytes: usize,
}

struct Entry<T> {
    key: SourceKey,
    value: Arc<T>,
    bytes: usize,
}

/// 排除进度和版本号；改变来源、提交人或登记指纹后必须重新加载并校验。
#[derive(PartialEq, Eq)]
struct SourceKey {
    job_id: String,
    fingerprint: CommandFingerprint,
}

impl SourceKey {
    fn new(job: &BackgroundJob) -> Self {
        Self {
            job_id: job.base.id.clone(),
            fingerprint: CommandFingerprint::from_parts([
                job.request_id.clone(),
                job.requested_by.clone(),
                job.domain_job_type.clone().unwrap_or_default(),
                job.domain_job_id.clone().unwrap_or_default(),
                job.input_file_asset_id.as_ref().map(ToString::to_string).unwrap_or_default(),
                job.request_fingerprint.as_ref().map(|value| value.as_str().to_string()).unwrap_or_default(),
                job.total_count.to_string(),
            ]),
        }
    }
}

impl<T> ImportSourceCache<T> {
    /// 创建同时限制任务数和输入字节数的缓存。
    ///
    /// # 参数
    /// * `max_bytes` - 所有常驻输入的合计字节预算。
    /// # 返回
    /// 返回空缓存。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self { entries: Mutex::new(VecDeque::new()), max_bytes }
    }

    /// 命中时共享来源；未命中时串行加载，失败或取消不会留下半成品。
    ///
    /// # 参数
    /// * `job` - 已读取并认领的当前任务，不可变字段参与缓存身份。
    /// * `load` - 加载、校验输入并返回来源及输入字节数；仅未命中时调用。
    /// # 返回
    /// 返回共享输入。超预算输入仅供本切片使用；容量不足时淘汰最近最少使用项。
    /// # 错误
    /// 原样传播加载错误，下一切片可重试。锁只覆盖输入准备，不覆盖业务行执行。
    pub(crate) async fn get_or_load<F, Fut>(&self, job: &BackgroundJob, load: F) -> Result<Arc<T>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(T, usize)>>,
    {
        let key = SourceKey::new(job);
        let mut entries = self.entries.lock().await;
        if let Some(index) = entries.iter().position(|entry| entry.key == key) {
            let entry = entries.remove(index).expect("已找到缓存项");
            let value = entry.value.clone();
            entries.push_back(entry);
            return Ok(value);
        }
        entries.retain(|entry| entry.key.job_id != key.job_id);
        let (value, bytes) = load().await?;
        let value = Arc::new(value);
        if bytes <= self.max_bytes {
            let mut used = entries.iter().map(|entry| entry.bytes).sum::<usize>();
            while entries.len() >= IMPORT_JOBS_PER_POLL as usize || used > self.max_bytes - bytes {
                if let Some(old) = entries.pop_front() {
                    used -= old.bytes;
                }
            }
            entries.push_back(Entry { key, value: value.clone(), bytes });
        }
        Ok(value)
    }

    /// 每轮查询后只保留本轮待执行任务，取消或已终结任务在此释放。
    ///
    /// # 参数
    /// * `jobs` - 本轮查询到的待执行任务。
    /// # 返回
    /// 无返回值；移除不再匹配的缓存引用。
    /// # 错误
    /// 不返回错误。
    pub(crate) async fn retain_jobs(&self, jobs: &[BackgroundJob]) {
        let keys = jobs.iter().map(SourceKey::new).collect::<Vec<_>>();
        self.entries.lock().await.retain(|entry| keys.contains(&entry.key));
    }

    /// 单任务完成或不再允许执行时立即释放来源。
    ///
    /// # 参数
    /// * `job_id` - 不再需要缓存的任务身份。
    /// # 返回
    /// 无返回值；正在执行的切片仍可使用自己的引用。
    /// # 错误
    /// 不返回错误。
    pub(crate) async fn remove(&self, job_id: &str) {
        self.entries.lock().await.retain(|entry| entry.key.job_id != job_id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use erp_core::ids::{BackgroundJobId, FileAssetId};
    use erp_support::{BackgroundJobData, JobType};

    use super::*;
    use crate::Error;

    fn fixture() -> BackgroundJob {
        let mut job = BackgroundJob::new(
            BackgroundJobId::new("job"),
            BackgroundJobData {
                job_no: "IMPORT-1".into(),
                job_type: JobType::Import,
                domain_job_type: Some("PRODUCT_IMPORT".into()),
                domain_job_id: Some("source".into()),
                selection_snapshot_id: None,
                requested_by: "admin".into(),
                request_id: "request".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 1000,
            },
        )
        .unwrap();
        job.attach_request_fingerprint(CommandFingerprint::from_parts(["payload".into()])).unwrap();
        job
    }

    #[tokio::test]
    async fn progress_and_version_changes_reuse_the_same_input() {
        let cache = ImportSourceCache::new(100);
        let mut job = fixture();
        let first = cache.get_or_load(&job, || async { Ok((vec![1, 2, 3], 3)) }).await.unwrap();
        job.base.version += 1;
        job.processed_count += 1;
        cache.retain_jobs(&[job.clone()]).await;
        let next = cache.get_or_load(&job, || async { panic!("输入不应重新读取") }).await.unwrap();
        assert!(Arc::ptr_eq(&first, &next));
        assert_eq!(*next, [1, 2, 3]);
    }

    #[tokio::test]
    async fn changed_input_or_owner_never_reuses_cached_data() {
        let job = fixture();
        let changes: [fn(&mut BackgroundJob); 7] = [
            |job| job.requested_by = "another-account".into(),
            |job| job.request_id = "another-request".into(),
            |job| job.domain_job_type = Some("another-type".into()),
            |job| job.domain_job_id = Some("another-source".into()),
            |job| job.input_file_asset_id = Some(FileAssetId::new("another-file")),
            |job| job.request_fingerprint = None,
            |job| job.total_count += 1,
        ];
        for change in changes {
            let cache = ImportSourceCache::new(100);
            cache.get_or_load(&job, || async { Ok(("original", 1)) }).await.unwrap();
            let mut changed = job.clone();
            change(&mut changed);
            let next = cache.get_or_load(&changed, || async { Ok(("revalidated", 1)) }).await.unwrap();
            assert_eq!(*next, "revalidated");
            assert_eq!(cache.entries.lock().await.len(), 1);
        }
    }

    #[tokio::test]
    async fn failed_load_is_retried_without_reusing_partial_data() {
        let cache = ImportSourceCache::new(100);
        let job = fixture();
        let error = cache
            .get_or_load(&job, || async { Err(Error::Internal("storage unavailable".into())) })
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "storage unavailable"));
        let next = cache.get_or_load(&job, || async { Ok(("recovered", 1)) }).await.unwrap();
        assert_eq!(*next, "recovered");
    }

    #[tokio::test]
    async fn byte_budget_evicts_least_recently_used_input_and_oversized_input_is_not_retained() {
        let cache = ImportSourceCache::new(10);
        let first = fixture();
        let mut second = first.clone();
        second.base.id = "second".into();
        let mut third = first.clone();
        third.base.id = "third".into();
        cache.get_or_load(&first, || async { Ok(("first", 5)) }).await.unwrap();
        cache.get_or_load(&second, || async { Ok(("second", 5)) }).await.unwrap();
        cache.get_or_load(&first, || async { panic!("缓存应命中") }).await.unwrap();
        cache.get_or_load(&third, || async { Ok(("third", 5)) }).await.unwrap();
        cache.get_or_load(&first, || async { panic!("近期使用项应保留") }).await.unwrap();
        let next = cache.get_or_load(&second, || async { Ok(("reloaded", 11)) }).await.unwrap();
        assert_eq!(*next, "reloaded");
        let next = cache.get_or_load(&second, || async { Ok(("oversized-reloaded", 11)) }).await.unwrap();
        assert_eq!(*next, "oversized-reloaded");
        assert_eq!(cache.entries.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn entry_limit_and_poll_cleanup_release_old_sources() {
        let cache = ImportSourceCache::new(100);
        let mut job = fixture();
        let first = cache.get_or_load(&job, || async { Ok(("first", 1)) }).await.unwrap();
        let old = Arc::downgrade(&first);
        drop(first);
        for number in 0..IMPORT_JOBS_PER_POLL {
            job.base.id = number.to_string();
            cache.get_or_load(&job, || async { Ok(("next", 1)) }).await.unwrap();
        }
        assert!(old.upgrade().is_none());
        assert_eq!(cache.entries.lock().await.len(), IMPORT_JOBS_PER_POLL as usize);
        cache.retain_jobs(&[job.clone()]).await;
        assert_eq!(cache.entries.lock().await.len(), 1);
        cache.remove(&job.base.id).await;
        assert!(cache.entries.lock().await.is_empty());
        cache.get_or_load(&job, || async { Ok(("next", 1)) }).await.unwrap();
        cache.retain_jobs(&[]).await;
        assert!(cache.entries.lock().await.is_empty());
    }

    #[tokio::test]
    async fn concurrent_slices_share_one_successful_load() {
        let cache = ImportSourceCache::new(100);
        let job = fixture();
        let loads = AtomicUsize::new(0);
        let load = || async {
            loads.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(("source", 1))
        };
        let (first, second) = tokio::join!(cache.get_or_load(&job, load), cache.get_or_load(&job, load));
        assert!(Arc::ptr_eq(&first.unwrap(), &second.unwrap()));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}
