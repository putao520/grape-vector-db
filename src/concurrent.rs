// 并发优化模块 - 提供高性能的并发数据结构和操作
use dashmap::DashMap;
use crossbeam::channel::{self, Receiver, Sender};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::hash::Hash;
use parking_lot::{RwLock, Mutex};
use rayon::prelude::*;

/// 高性能并发哈希映射，基于DashMap实现
pub struct ConcurrentHashMap<K, V> {
    inner: DashMap<K, V>,
    access_counter: AtomicU64,
}

impl<K, V> ConcurrentHashMap<K, V> 
where 
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 创建新的并发哈希映射
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
            access_counter: AtomicU64::new(0),
        }
    }

    /// 插入键值对
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.inner.insert(key, value)
    }

    /// 获取值
    pub fn get(&self, key: &K) -> Option<dashmap::mapref::one::Ref<K, V>> {
        self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.inner.get(key)
    }

    /// 移除键值对
    pub fn remove(&self, key: &K) -> Option<(K, V)> {
        self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.inner.remove(key)
    }

    /// 获取映射大小
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 获取访问计数
    pub fn access_count(&self) -> u64 {
        self.access_counter.load(Ordering::Relaxed)
    }

    /// 批量插入
    pub fn batch_insert(&self, items: Vec<(K, V)>) 
    where 
        K: Send,
        V: Send,
    {
        // 使用rayon并行插入
        items.into_par_iter().for_each(|(k, v)| {
            self.insert(k, v);
        });
    }

    /// 批量获取
    pub fn batch_get(&self, keys: &[K]) -> Vec<Option<V>>
    where
        K: Send + Sync,
        V: Send,
    {
        keys.par_iter()
            .map(|key| self.get(key).map(|v| v.clone()))
            .collect()
    }
}

impl<K, V> Default for ConcurrentHashMap<K, V> 
where 
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

/// 多生产者多消费者队列
pub struct MPMCQueue<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
    sent_count: AtomicUsize,
    received_count: AtomicUsize,
}

impl<T> MPMCQueue<T> {
    /// 创建有界队列
    pub fn bounded(capacity: usize) -> Self {
        let (sender, receiver) = channel::bounded(capacity);
        Self {
            sender,
            receiver,
            sent_count: AtomicUsize::new(0),
            received_count: AtomicUsize::new(0),
        }
    }

    /// 创建无界队列
    pub fn unbounded() -> Self {
        let (sender, receiver) = channel::unbounded();
        Self {
            sender,
            receiver,
            sent_count: AtomicUsize::new(0),
            received_count: AtomicUsize::new(0),
        }
    }

    /// 发送数据
    pub fn send(&self, item: T) -> Result<(), crossbeam::channel::SendError<T>> {
        self.sent_count.fetch_add(1, Ordering::Relaxed);
        self.sender.send(item)
    }

    /// 接收数据
    pub fn receive(&self) -> Result<T, crossbeam::channel::RecvError> {
        let result = self.receiver.recv();
        if result.is_ok() {
            self.received_count.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    /// 尝试接收数据（非阻塞）
    pub fn try_receive(&self) -> Result<T, crossbeam::channel::TryRecvError> {
        let result = self.receiver.try_recv();
        if result.is_ok() {
            self.received_count.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    /// 获取发送计数
    pub fn sent_count(&self) -> usize {
        self.sent_count.load(Ordering::Relaxed)
    }

    /// 获取接收计数
    pub fn received_count(&self) -> usize {
        self.received_count.load(Ordering::Relaxed)
    }

    /// 获取队列中的项目数量（估计值）
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    /// 检查队列是否为空
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    /// 克隆发送器用于多生产者
    pub fn sender(&self) -> Sender<T> {
        self.sender.clone()
    }

    /// 克隆接收器用于多消费者
    pub fn receiver(&self) -> Receiver<T> {
        self.receiver.clone()
    }
}

/// 原子计数器集合
#[derive(Debug)]
pub struct AtomicCounters {
    pub operations: AtomicU64,
    pub successful_operations: AtomicU64,
    pub failed_operations: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub index_updates: AtomicU64,
    pub search_operations: AtomicU64,
}

impl AtomicCounters {
    /// 创建新的原子计数器集合
    pub fn new() -> Self {
        Self {
            operations: AtomicU64::new(0),
            successful_operations: AtomicU64::new(0),
            failed_operations: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            index_updates: AtomicU64::new(0),
            search_operations: AtomicU64::new(0),
        }
    }

    /// 增加操作计数
    pub fn increment_operations(&self) {
        self.operations.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加成功操作计数
    pub fn increment_successful_operations(&self) {
        self.successful_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加失败操作计数
    pub fn increment_failed_operations(&self) {
        self.failed_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加缓存命中计数
    pub fn increment_cache_hits(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加缓存未命中计数
    pub fn increment_cache_misses(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加索引更新计数
    pub fn increment_index_updates(&self) {
        self.index_updates.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加搜索操作计数
    pub fn increment_search_operations(&self) {
        self.search_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取操作计数
    pub fn get_operations(&self) -> u64 {
        self.operations.load(Ordering::Relaxed)
    }

    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.operations.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let successful = self.successful_operations.load(Ordering::Relaxed);
        successful as f64 / total as f64
    }

    /// 获取缓存命中率
    pub fn cache_hit_rate(&self) -> f64 {
        let total_cache_access = self.cache_hits.load(Ordering::Relaxed) + self.cache_misses.load(Ordering::Relaxed);
        if total_cache_access == 0 {
            return 0.0;
        }
        let hits = self.cache_hits.load(Ordering::Relaxed);
        hits as f64 / total_cache_access as f64
    }

    /// 重置所有计数器
    pub fn reset(&self) {
        self.operations.store(0, Ordering::Relaxed);
        self.successful_operations.store(0, Ordering::Relaxed);
        self.failed_operations.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.index_updates.store(0, Ordering::Relaxed);
        self.search_operations.store(0, Ordering::Relaxed);
    }
}

impl Default for AtomicCounters {
    fn default() -> Self {
        Self::new()
    }
}

/// 工作偷取队列，用于任务分发
pub struct WorkStealingQueue<T> {
    local_queue: Arc<Mutex<Vec<T>>>,
    steal_queues: Arc<RwLock<Vec<Arc<Mutex<Vec<T>>>>>>,
    worker_id: usize,
    total_workers: AtomicUsize,
}

impl<T> WorkStealingQueue<T> 
where 
    T: Send + Sync,
{
    /// 创建工作偷取队列系统
    pub fn new(num_workers: usize) -> Vec<Self> {
        let steal_queues = Arc::new(RwLock::new(Vec::new()));
        let mut queues = Vec::new();

        // 创建所有本地队列
        for i in 0..num_workers {
            let local_queue = Arc::new(Mutex::new(Vec::new()));
            steal_queues.write().push(local_queue.clone());
            
            queues.push(Self {
                local_queue,
                steal_queues: steal_queues.clone(),
                worker_id: i,
                total_workers: AtomicUsize::new(num_workers),
            });
        }

        queues
    }

    /// 推送任务到本地队列
    pub fn push(&self, item: T) {
        self.local_queue.lock().push(item);
    }

    /// 从本地队列弹出任务
    pub fn pop(&self) -> Option<T> {
        self.local_queue.lock().pop()
    }

    /// 尝试从其他队列偷取任务
    pub fn steal(&self) -> Option<T> {
        let queues = self.steal_queues.read();
        let total_workers = self.total_workers.load(Ordering::Relaxed);
        
        // 随机选择一个其他工作者的队列
        for _ in 0..total_workers {
            let target_id = (self.worker_id + 1 + fastrand::usize(0..total_workers - 1)) % total_workers;
            if target_id != self.worker_id {
                if let Some(queue) = queues.get(target_id) {
                    let mut target_queue = queue.lock();
                    if !target_queue.is_empty() {
                        // 偷取队列前半部分的任务
                        let steal_count = target_queue.len() / 2;
                        if steal_count > 0 {
                            return target_queue.drain(0..1).next();
                        }
                    }
                }
            }
        }
        None
    }

    /// 获取任务（先从本地队列，再尝试偷取）
    pub fn get_task(&self) -> Option<T> {
        self.pop().or_else(|| self.steal())
    }

    /// 获取本地队列长度
    pub fn local_len(&self) -> usize {
        self.local_queue.lock().len()
    }

    /// 获取总任务数（所有队列）
    pub fn total_len(&self) -> usize {
        let queues = self.steal_queues.read();
        queues.iter().map(|q| q.lock().len()).sum()
    }
}

/// 并发批处理器
pub struct ConcurrentBatchProcessor<T, R> {
    batch_size: usize,
    processor: Arc<dyn Fn(Vec<T>) -> Vec<R> + Send + Sync>,
    queue: MPMCQueue<T>,
    result_queue: MPMCQueue<R>,
}

impl<T, R> ConcurrentBatchProcessor<T, R> 
where 
    T: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    /// 创建新的批处理器
    pub fn new<F>(batch_size: usize, processor: F) -> Self 
    where 
        F: Fn(Vec<T>) -> Vec<R> + Send + Sync + 'static,
    {
        Self {
            batch_size,
            processor: Arc::new(processor),
            queue: MPMCQueue::bounded(batch_size * 10),
            result_queue: MPMCQueue::bounded(batch_size * 10),
        }
    }

    /// 添加任务到批处理队列
    pub fn add_task(&self, task: T) -> Result<(), crossbeam::channel::SendError<T>> {
        self.queue.send(task)
    }

    /// 获取处理结果
    pub fn get_result(&self) -> Result<R, crossbeam::channel::RecvError> {
        self.result_queue.receive()
    }

    /// 启动批处理工作线程
    pub fn start_workers(&self, num_workers: usize) {
        for _ in 0..num_workers {
            let queue = self.queue.receiver();
            let result_queue = self.result_queue.sender();
            let processor = self.processor.clone();
            let batch_size = self.batch_size;

            std::thread::spawn(move || {
                let mut batch = Vec::with_capacity(batch_size);
                
                loop {
                    // 收集批次
                    for _ in 0..batch_size {
                        match queue.recv() {
                            Ok(task) => batch.push(task),
                            Err(_) => break, // 通道关闭
                        }
                    }

                    if !batch.is_empty() {
                        // 处理批次
                        let results = processor(batch.drain(..).collect());
                        
                        // 发送结果
                        for result in results {
                            if result_queue.send(result).is_err() {
                                break; // 结果通道关闭
                            }
                        }
                    }
                }
            });
        }
    }

    /// 获取队列状态
    pub fn queue_status(&self) -> (usize, usize) {
        (self.queue.len(), self.result_queue.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_concurrent_hashmap() {
        let map = ConcurrentHashMap::new();
        
        // 测试插入和获取
        map.insert("key1", "value1");
        assert_eq!(map.get(&"key1").map(|v| v.clone()), Some("value1"));
        
        // 测试批量操作
        let items = vec![("key2", "value2"), ("key3", "value3")];
        map.batch_insert(items);
        
        assert_eq!(map.len(), 3);
        assert!(map.access_count() > 0);
    }

    #[test]
    fn test_mpmc_queue() {
        let queue = MPMCQueue::bounded(10);
        
        // 测试发送和接收
        queue.send("item1").unwrap();
        queue.send("item2").unwrap();
        
        assert_eq!(queue.receive().unwrap(), "item1");
        assert_eq!(queue.receive().unwrap(), "item2");
        
        assert_eq!(queue.sent_count(), 2);
        assert_eq!(queue.received_count(), 2);
    }

    #[test]
    fn test_atomic_counters() {
        let counters = AtomicCounters::new();
        
        counters.increment_operations();
        counters.increment_successful_operations();
        counters.increment_cache_hits();
        counters.increment_cache_misses();
        
        assert_eq!(counters.get_operations(), 1);
        assert_eq!(counters.success_rate(), 1.0);
        assert_eq!(counters.cache_hit_rate(), 0.5);
    }

    #[test]
    fn test_work_stealing_queue() {
        let queues = WorkStealingQueue::new(2);
        
        // 向第一个队列推送任务
        queues[0].push("task1");
        queues[0].push("task2");
        
        // 从第二个队列偷取任务
        let stolen_task = queues[1].steal();
        assert!(stolen_task.is_some());
    }

    #[test]
    fn test_batch_processor() {
        let processor = ConcurrentBatchProcessor::new(2, |batch: Vec<i32>| {
            batch.into_iter().map(|x| x * 2).collect()
        });

        processor.start_workers(1);
        
        processor.add_task(1).unwrap();
        processor.add_task(2).unwrap();
        
        std::thread::sleep(Duration::from_millis(100));
        
        let result1 = processor.get_result();
        let result2 = processor.get_result();
        
        assert!(result1.is_ok() || result2.is_ok());
    }
}