// Advanced Storage Engine Module (Sled-based)
// Week 9-10: Storage Engine Upgrade

use crate::errors::{Result, VectorDbError};
use crate::types::Point;
use serde::{Deserialize, Serialize};
use sled::{
    transaction::{TransactionResult, Transactional},
    Db, Tree,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Advanced storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedStorageConfig {
    /// Database path
    pub db_path: PathBuf,
    /// Backup path
    pub backup_path: Option<PathBuf>,
    /// Enable compression
    pub enable_compression: bool,
    /// Cache size in bytes
    pub cache_size: usize,
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
    /// Enable checksums
    pub enable_checksums: bool,
    /// Maximum number of background threads
    pub max_background_threads: usize,
}

impl Default for AdvancedStorageConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./grape_vector_db"),
            backup_path: None,
            enable_compression: false, // Disable compression to avoid Sled feature issues
            cache_size: 512 * 1024 * 1024, // 512MB
            flush_interval_ms: 1000,
            enable_checksums: true,
            max_background_threads: 4,
        }
    }
}

/// Column families for different data types
pub struct ColumnFamilies;

impl ColumnFamilies {
    pub const VECTORS: &'static str = "vectors";
    pub const METADATA: &'static str = "metadata";
    pub const INDEX: &'static str = "index";
    pub const SPARSE: &'static str = "sparse";
    pub const QUANTIZED: &'static str = "quantized";
    pub const STATS: &'static str = "stats";
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub estimated_keys: u64,
    pub total_size: u64,
    pub live_data_size: u64,
    pub compression_ratio: f64,
    pub cache_hit_rate: f64,
    pub last_backup_time: Option<u64>,
}

/// Advanced storage engine using Sled
pub struct AdvancedStorage {
    db: Db,
    config: AdvancedStorageConfig,
    trees: HashMap<String, Tree>,
    stats: Arc<parking_lot::RwLock<StorageStats>>,
}

impl AdvancedStorage {
    /// Create a new advanced storage instance
    pub fn new(config: AdvancedStorageConfig) -> Result<Self> {
        // Configure Sled database
        let sled_config = sled::Config::default()
            .path(&config.db_path)
            .cache_capacity(config.cache_size as u64)
            .flush_every_ms(Some(config.flush_interval_ms));
        // Disable compression for now to avoid Sled feature issues
        // .use_compression(config.enable_compression);

        // Removing checksum-based compression for now
        // if config.enable_checksums {
        //     sled_config = sled_config.use_compression(true);
        // }

        let db = sled_config
            .open()
            .map_err(|e| VectorDbError::StorageError(format!("Failed to open database: {}", e)))?;

        // Initialize column families (trees in Sled)
        let mut trees = HashMap::new();
        let cf_names = [
            ColumnFamilies::VECTORS,
            ColumnFamilies::METADATA,
            ColumnFamilies::INDEX,
            ColumnFamilies::SPARSE,
            ColumnFamilies::QUANTIZED,
            ColumnFamilies::STATS,
        ];

        for cf_name in cf_names.iter() {
            let tree = db.open_tree(cf_name).map_err(|e| {
                VectorDbError::StorageError(format!("Failed to open tree {}: {}", cf_name, e))
            })?;
            trees.insert(cf_name.to_string(), tree);
        }

        let stats = Arc::new(parking_lot::RwLock::new(StorageStats {
            estimated_keys: 0,
            total_size: 0,
            live_data_size: 0,
            compression_ratio: 1.0,
            cache_hit_rate: 0.0,
            last_backup_time: None,
        }));

        Ok(Self {
            db,
            config,
            trees,
            stats,
        })
    }

    /// Get a tree (column family) by name
    pub fn get_tree(&self, name: &str) -> Result<&Tree> {
        self.trees
            .get(name)
            .ok_or_else(|| VectorDbError::StorageError(format!("Tree {} not found", name)))
    }

    /// Store a vector in the database
    pub fn store_vector(&self, point: &Point) -> Result<()> {
        let vectors_tree = self.get_tree(ColumnFamilies::VECTORS)?;
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;

        // Serialize vector data
        let vector_data = bincode::serialize(&point.vector).map_err(|e| {
            VectorDbError::SerializationError(format!("Failed to serialize vector: {}", e))
        })?;

        // Serialize metadata
        let payload_json = serde_json::to_string(&point.payload).map_err(|e| {
            VectorDbError::SerializationError(format!("Failed to serialize payload: {}", e))
        })?;

        let metadata = VectorMetadata {
            id: point.id.clone(),
            dimension: point.vector.len(),
            payload_json,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            updated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let metadata_data = bincode::serialize(&metadata).map_err(|e| {
            VectorDbError::SerializationError(format!("Failed to serialize metadata: {}", e))
        })?;

        // Use transaction for atomic writes
        let result: TransactionResult<(), ()> =
            (vectors_tree, metadata_tree).transaction(|(vectors_tx, metadata_tx)| {
                vectors_tx.insert(point.id.as_bytes(), vector_data.clone())?;
                metadata_tx.insert(point.id.as_bytes(), metadata_data.clone())?;
                Ok(())
            });

        result.map_err(|e| VectorDbError::StorageError(format!("Transaction failed: {:?}", e)))?;

        // Update statistics
        self.update_stats();

        Ok(())
    }

    /// Retrieve a vector from the database
    pub fn get_vector(&self, id: &str) -> Result<Option<Point>> {
        let vectors_tree = self.get_tree(ColumnFamilies::VECTORS)?;
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;

        // Get vector data
        let vector_data = vectors_tree
            .get(id.as_bytes())
            .map_err(|e| VectorDbError::StorageError(format!("Failed to get vector: {}", e)))?;

        let metadata_data = metadata_tree
            .get(id.as_bytes())
            .map_err(|e| VectorDbError::StorageError(format!("Failed to get metadata: {}", e)))?;

        match (vector_data, metadata_data) {
            (Some(vector_bytes), Some(metadata_bytes)) => {
                let vector: Vec<f32> = bincode::deserialize(&vector_bytes).map_err(|e| {
                    VectorDbError::SerializationError(format!(
                        "Failed to deserialize vector: {}",
                        e
                    ))
                })?;

                let metadata: VectorMetadata =
                    bincode::deserialize(&metadata_bytes).map_err(|e| {
                        VectorDbError::SerializationError(format!(
                            "Failed to deserialize metadata: {}",
                            e
                        ))
                    })?;

                let payload: HashMap<String, serde_json::Value> =
                    serde_json::from_str(&metadata.payload_json).map_err(|e| {
                        VectorDbError::SerializationError(format!(
                            "Failed to deserialize payload JSON: {}",
                            e
                        ))
                    })?;

                Ok(Some(Point {
                    id: metadata.id,
                    vector,
                    payload,
                }))
            }
            _ => Ok(None),
        }
    }

    /// Delete a vector from the database
    pub fn delete_vector(&self, id: &str) -> Result<bool> {
        let vectors_tree = self.get_tree(ColumnFamilies::VECTORS)?;
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;

        let result: TransactionResult<bool, ()> =
            (vectors_tree, metadata_tree).transaction(|(vectors_tx, metadata_tx)| {
                let vector_existed = vectors_tx.remove(id.as_bytes())?.is_some();
                let metadata_existed = metadata_tx.remove(id.as_bytes())?.is_some();
                Ok(vector_existed || metadata_existed)
            });

        let deleted = result.map_err(|e| {
            VectorDbError::StorageError(format!("Delete transaction failed: {:?}", e))
        })?;

        if deleted {
            self.update_stats();
        }

        Ok(deleted)
    }

    /// Create a backup of the database
    pub fn create_backup(&self) -> Result<String> {
        if let Some(backup_path) = &self.config.backup_path {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let backup_id = format!("backup_{}", timestamp);
            let backup_dir = backup_path.join(&backup_id);

            // Create backup directory
            std::fs::create_dir_all(&backup_dir).map_err(|e| {
                VectorDbError::StorageError(format!("Failed to create backup directory: {}", e))
            })?;

            // Export database to backup directory
            for (key, value, _) in self.db.export() {
                let backup_file = backup_dir.join(format!(
                    "{:x}.backup",
                    key.iter()
                        .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64))
                ));
                std::fs::write(backup_file, &value).map_err(|e| {
                    VectorDbError::StorageError(format!("Failed to write backup file: {}", e))
                })?;
            }

            // Update backup timestamp in stats
            {
                let mut stats = self.stats.write();
                stats.last_backup_time = Some(timestamp);
            }

            Ok(backup_id)
        } else {
            Err(VectorDbError::StorageError(
                "Backup path not configured".to_string(),
            ))
        }
    }

    /// Create a checkpoint (snapshot) of the database
    pub fn create_checkpoint<P: AsRef<Path>>(&self, checkpoint_path: P) -> Result<()> {
        let checkpoint_dir = checkpoint_path.as_ref();

        // Create checkpoint directory
        std::fs::create_dir_all(checkpoint_dir).map_err(|e| {
            VectorDbError::StorageError(format!("Failed to create checkpoint directory: {}", e))
        })?;

        // Flush all pending writes
        self.db
            .flush()
            .map_err(|e| VectorDbError::StorageError(format!("Failed to flush database: {}", e)))?;

        // Use Sled's export functionality instead of copying files
        // This avoids file locking issues
        let checkpoint_file = checkpoint_dir.join("checkpoint.db");
        let mut checkpoint_data = Vec::new();

        for (key, value, _) in self.db.export() {
            checkpoint_data.push((key.to_vec(), value.to_vec()));
        }

        // Write checkpoint data as a simple format
        let serialized = bincode::serialize(&checkpoint_data).map_err(|e| {
            VectorDbError::StorageError(format!("Failed to serialize checkpoint: {}", e))
        })?;

        std::fs::write(checkpoint_file, serialized).map_err(|e| {
            VectorDbError::StorageError(format!("Failed to write checkpoint file: {}", e))
        })?;

        Ok(())
    }

    /// Compact the database to reclaim space
    pub fn compact(&self) -> Result<()> {
        // Sled doesn't have explicit compaction, but we can trigger cleanup
        self.db.flush().map_err(|e| {
            VectorDbError::StorageError(format!("Failed to flush during compaction: {}", e))
        })?;

        // Update statistics after compaction
        self.update_stats();

        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> StorageStats {
        self.update_stats();
        self.stats.read().clone()
    }

    /// 预热缓存
    pub async fn warmup_cache(&self) -> Result<()> {
        tracing::info!("Starting cache warmup...");
        let start = Instant::now();

        // 预加载一些常用数据到缓存
        // 这里可以实现具体的预热逻辑，比如：
        // 1. 预加载最近访问的向量
        // 2. 预加载索引数据
        // 3. 预加载元数据

        // 简单实现：遍历一部分数据来预热缓存
        let vectors_tree = self.get_tree(ColumnFamilies::VECTORS)?;
        let mut count = 0;
        for (_, _) in vectors_tree.iter().take(1000).flatten() {
            // 预热前1000个向量
            count += 1;
        }

        tracing::info!(
            "Cache warmup completed in {:?}, preloaded {} items",
            start.elapsed(),
            count
        );
        Ok(())
    }

    /// 刷新数据到磁盘
    pub async fn flush(&self) -> Result<()> {
        self.db
            .flush_async()
            .await
            .map_err(|e| VectorDbError::StorageError(format!("Failed to flush: {}", e)))?;
        Ok(())
    }

    /// 同步数据
    pub async fn sync(&self) -> Result<()> {
        // Sled的flush_async已经包含了同步操作
        self.flush().await
    }

    /// List all vector IDs
    pub fn list_vector_ids(&self) -> Result<Vec<String>> {
        let vectors_tree = self.get_tree(ColumnFamilies::VECTORS)?;
        let mut ids = Vec::new();

        for result in vectors_tree.iter() {
            match result {
                Ok((key, _)) => {
                    let id = String::from_utf8_lossy(&key).to_string();
                    ids.push(id);
                }
                Err(e) => {
                    return Err(VectorDbError::StorageError(format!(
                        "Failed to iterate vectors: {}",
                        e
                    )));
                }
            }
        }

        Ok(ids)
    }

    /// Batch store multiple vectors
    pub async fn batch_store_vectors(&self, points: Vec<Point>) -> Result<()> {
        let vectors_tree = self.get_tree(ColumnFamilies::VECTORS)?;
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;

        let result: TransactionResult<(), ()> =
            (vectors_tree, metadata_tree).transaction(|(vectors_tx, metadata_tx)| {
                for point in &points {
                    // Serialize vector data
                    let vector_data = match bincode::serialize(&point.vector) {
                        Ok(data) => data,
                        Err(_) => {
                            return Err(sled::transaction::ConflictableTransactionError::Abort(()))
                        }
                    };

                    // Serialize metadata
                    let payload_json = match serde_json::to_string(&point.payload) {
                        Ok(json) => json,
                        Err(_) => {
                            return Err(sled::transaction::ConflictableTransactionError::Abort(()))
                        }
                    };

                    let metadata = VectorMetadata {
                        id: point.id.clone(),
                        dimension: point.vector.len(),
                        payload_json,
                        created_at: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        updated_at: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };

                    let metadata_data = match bincode::serialize(&metadata) {
                        Ok(data) => data,
                        Err(_) => {
                            return Err(sled::transaction::ConflictableTransactionError::Abort(()))
                        }
                    };

                    vectors_tx.insert(point.id.as_bytes(), vector_data)?;
                    metadata_tx.insert(point.id.as_bytes(), metadata_data)?;
                }
                Ok(())
            });

        result.map_err(|e| {
            VectorDbError::StorageError(format!("Batch transaction failed: {:?}", e))
        })?;

        self.update_stats();
        Ok(())
    }

    /// Update internal statistics
    fn update_stats(&self) {
        if let Ok(vectors_tree) = self.get_tree(ColumnFamilies::VECTORS) {
            let mut stats = self.stats.write();

            // Estimate number of keys
            stats.estimated_keys = vectors_tree.len() as u64;

            // Calculate approximate sizes
            let mut total_size = 0u64;
            for (key, value) in vectors_tree.iter().flatten() {
                total_size += key.len() as u64 + value.len() as u64;
            }

            stats.total_size = total_size;
            stats.live_data_size = total_size; // Sled handles compression internally

            // Estimate compression ratio (simplified)
            if stats.total_size > 0 {
                stats.compression_ratio = if self.config.enable_compression {
                    0.7
                } else {
                    1.0
                };
            }

            // Cache hit rate (simplified estimation)
            stats.cache_hit_rate = 0.85; // Sled manages cache internally
        }
    }

    /// Put raw data (generic method for distributed systems)
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;
        metadata_tree
            .insert(key, value)
            .map_err(|e| VectorDbError::StorageError(format!("Failed to put data: {}", e)))?;
        Ok(())
    }

    /// Get raw data (generic method for distributed systems)
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;
        let result = metadata_tree
            .get(key)
            .map_err(|e| VectorDbError::StorageError(format!("Failed to get data: {}", e)))?;
        Ok(result.map(|ivec| ivec.to_vec()))
    }

    /// Delete raw data (generic method for distributed systems)
    pub fn delete(&self, key: &[u8]) -> Result<bool> {
        let metadata_tree = self.get_tree(ColumnFamilies::METADATA)?;
        let removed = metadata_tree
            .remove(key)
            .map_err(|e| VectorDbError::StorageError(format!("Failed to delete data: {}", e)))?;
        Ok(removed.is_some())
    }
}

/// Vector metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorMetadata {
    pub id: String,
    pub dimension: usize,
    pub payload_json: String, // Store payload as JSON string
    pub created_at: u64,
    pub updated_at: u64,
}

/// Helper function to copy directory recursively
#[allow(dead_code)]
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
