// Binary Quantization Module
// Week 5-6: Binary Quantization Implementation

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use bitvec::prelude::*;
use crate::errors::{Result, VectorDbError};

/// Binary quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryQuantizationConfig {
    /// Quantization threshold for converting float to binary
    pub threshold: f32,
    /// Enable SIMD optimizations
    pub enable_simd: bool,
    /// Rescore ratio for multi-stage search
    pub rescore_ratio: f32,
    /// Cache quantized vectors in memory
    pub enable_cache: bool,
}

impl Default for BinaryQuantizationConfig {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            enable_simd: true,
            rescore_ratio: 0.1,
            enable_cache: true,
        }
    }
}

/// Binary quantized vector representation
#[derive(Debug, Clone)]
pub struct BinaryVector {
    /// Binary data as packed bits
    pub data: BitVec<u8, bitvec::order::Msb0>,
    /// Original dimension before quantization
    pub dimension: usize,
}

impl BinaryVector {
    /// Create a new binary vector from raw data
    pub fn new(data: BitVec<u8, bitvec::order::Msb0>, dimension: usize) -> Self {
        Self { data, dimension }
    }
    
    /// Get the number of bytes required to store this binary vector
    pub fn byte_size(&self) -> usize {
        self.dimension.div_ceil(8)
    }
    
    /// Convert to raw bytes for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        self.data.clone().into_vec()
    }
    
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>, dimension: usize) -> Self {
        let data = BitVec::from_vec(bytes);
        Self { data, dimension }
    }
}

/// Binary quantization engine
#[derive(Debug)]
pub struct BinaryQuantizer {
    config: BinaryQuantizationConfig,
    /// Cache for quantized vectors
    cache: Option<HashMap<String, BinaryVector>>,
}

impl BinaryQuantizer {
    /// Create a new binary quantizer
    pub fn new(config: BinaryQuantizationConfig) -> Self {
        let cache = if config.enable_cache {
            Some(HashMap::new())
        } else {
            None
        };
        
        Self { config, cache }
    }
    
    /// Quantize a float vector to binary
    pub fn quantize(&mut self, vector: &[f32]) -> Result<BinaryVector> {
        // Generate cache key if caching is enabled
        if let Some(ref mut cache) = self.cache {
            let cache_key = format!("{:?}", vector); // In production, use a proper hash
            
            // Check cache first
            if let Some(cached_result) = cache.get(&cache_key) {
                return Ok(cached_result.clone());
            }
            
            // Compute and cache result
            let mut binary_vec = BitVec::<u8, bitvec::order::Msb0>::with_capacity(vector.len());
            
            for &value in vector {
                binary_vec.push(value > self.config.threshold);
            }
            
            let result = BinaryVector::new(binary_vec, vector.len());
            
            // Store in cache (with size limit to prevent memory growth)
            if cache.len() < 10000 { // Limit cache size
                cache.insert(cache_key, result.clone());
            }
            
            Ok(result)
        } else {
            // No caching, compute directly
            let mut binary_vec = BitVec::<u8, bitvec::order::Msb0>::with_capacity(vector.len());
            
            for &value in vector {
                binary_vec.push(value > self.config.threshold);
            }
            
            Ok(BinaryVector::new(binary_vec, vector.len()))
        }
    }
    
    /// Quantize multiple vectors in batch
    pub fn quantize_batch(&mut self, vectors: &[Vec<f32>]) -> Result<Vec<BinaryVector>> {
        vectors.iter().map(|v| self.quantize(v)).collect()
    }
    
    /// Compute Hamming distance between two binary vectors using optimized method
    pub fn hamming_distance(&self, a: &BinaryVector, b: &BinaryVector) -> Result<f32> {
        if a.dimension != b.dimension {
            return Err(VectorDbError::InvalidVectorDimension);
        }
        
        let a_bytes = a.to_bytes();
        let b_bytes = b.to_bytes();
        
        // Use hamming library for optimized distance calculation
        let distance = hamming::distance(&a_bytes, &b_bytes);
        Ok(distance as f32)
    }
    
    /// Compute similarity score (1 - normalized hamming distance)
    pub fn similarity(&self, a: &BinaryVector, b: &BinaryVector) -> Result<f32> {
        let distance = self.hamming_distance(a, b)?;
        let max_distance = a.dimension as f32;
        Ok(1.0 - (distance / max_distance))
    }
    
    /// Multi-stage search: coarse search with binary vectors, then rescore with original
    pub fn multi_stage_search(
        &self,
        query_binary: &BinaryVector,
        candidates_binary: &[BinaryVector],
        original_query: &[f32],
        original_candidates: &[Vec<f32>],
    ) -> Result<Vec<(usize, f32)>> {
        if candidates_binary.len() != original_candidates.len() {
            return Err(VectorDbError::QuantizationError(
                "Mismatch between binary and original candidate counts".to_string()
            ));
        }
        
        // Stage 1: Fast binary search
        let mut binary_scores: Vec<(usize, f32)> = candidates_binary
            .iter()
            .enumerate()
            .map(|(idx, candidate)| {
                let score = self.similarity(query_binary, candidate)
                    .unwrap_or(0.0);
                (idx, score)
            })
            .collect();
        
        // Sort by binary similarity
        binary_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Stage 2: Rescore top candidates with original vectors
        let rescore_count = (candidates_binary.len() as f32 * self.config.rescore_ratio) as usize;
        let top_candidates = &binary_scores[..rescore_count.min(binary_scores.len())];
        
        let mut final_scores: Vec<(usize, f32)> = top_candidates
            .iter()
            .map(|(idx, _)| {
                let cosine_sim = self.cosine_similarity(original_query, &original_candidates[*idx]);
                (*idx, cosine_sim)
            })
            .collect();
        
        // Sort by cosine similarity
        final_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        Ok(final_scores)
    }
    
    /// Helper function to compute cosine similarity
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if self.config.enable_simd {
            // Fallback to manual implementation for now
            self.cosine_similarity_manual(a, b)
        } else {
            self.cosine_similarity_manual(a, b)
        }
    }
    
    /// Manual cosine similarity implementation
    fn cosine_similarity_manual(&self, a: &[f32], b: &[f32]) -> f32 {
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }

    /// Clear the quantization cache
    pub fn clear_cache(&mut self) {
        if let Some(ref mut cache) = self.cache {
            cache.clear();
        }
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> CacheStats {
        if let Some(ref cache) = self.cache {
            CacheStats {
                enabled: true,
                size: cache.len(),
                capacity: 10000,
            }
        } else {
            CacheStats {
                enabled: false,
                size: 0,
                capacity: 0,
            }
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub enabled: bool,
    pub size: usize,
    pub capacity: usize,
}

/// Performance metrics for binary quantization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationMetrics {
    /// Quantization time in milliseconds
    pub quantization_time_ms: f64,
    /// Search time in milliseconds
    pub search_time_ms: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: usize,
    /// Compression ratio (original size / quantized size)
    pub compression_ratio: f32,
    /// Accuracy compared to exact search
    pub accuracy: f32,
}

impl QuantizationMetrics {
    pub fn new() -> Self {
        Self {
            quantization_time_ms: 0.0,
            search_time_ms: 0.0,
            memory_usage_bytes: 0,
            compression_ratio: 1.0,
            accuracy: 1.0,
        }
    }
}

impl Default for QuantizationMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory mapped storage for binary vectors
#[derive(Debug)]
pub struct BinaryVectorStore {
    /// Configuration
    config: BinaryQuantizationConfig,
    /// In-memory storage for binary vectors
    vectors: Vec<BinaryVector>,
    /// Metadata for each vector
    metadata: Vec<String>,
}

impl BinaryVectorStore {
    /// Create a new binary vector store
    pub fn new(config: BinaryQuantizationConfig) -> Self {
        Self {
            config,
            vectors: Vec::new(),
            metadata: Vec::new(),
        }
    }
    
    /// Add a binary vector to the store
    pub fn add_vector(&mut self, vector: BinaryVector, id: String) -> Result<()> {
        self.vectors.push(vector);
        self.metadata.push(id);
        Ok(())
    }
    
    /// Get vector by index
    pub fn get_vector(&self, index: usize) -> Option<&BinaryVector> {
        self.vectors.get(index)
    }
    
    /// Get the number of vectors
    pub fn len(&self) -> usize {
        self.vectors.len()
    }
    
    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
    
    /// Get current configuration
    pub fn get_config(&self) -> &BinaryQuantizationConfig {
        &self.config
    }
    
    /// Validate vector based on configuration
    pub fn validate_vector(&self, vector: &BinaryVector) -> Result<()> {
        if vector.dimension == 0 {
            return Err(VectorDbError::InvalidVectorDimension);
        }
        
        // Additional validation based on config can be added here
        if self.config.enable_cache && vector.byte_size() > 1024 * 1024 {
            tracing::warn!("Large vector detected ({}MB), may impact cache performance", 
                vector.byte_size() / 1024 / 1024);
        }
        
        Ok(())
    }
    
    /// Calculate total memory usage
    pub fn memory_usage(&self) -> usize {
        self.vectors.iter().map(|v| v.byte_size()).sum::<usize>() +
        self.metadata.iter().map(|m| m.len()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_binary_quantization() {
        let config = BinaryQuantizationConfig::default();
        let mut quantizer = BinaryQuantizer::new(config);
        
        let vector = vec![0.5, -0.3, 0.8, -0.1, 0.2];
        let binary_vec = quantizer.quantize(&vector).unwrap();
        
        assert_eq!(binary_vec.dimension, 5);
        // Expected: [1, 0, 1, 0, 1] based on threshold 0.0
        assert_eq!(binary_vec.data.len(), 5);
    }
    
    #[test]
    fn test_hamming_distance() {
        let config = BinaryQuantizationConfig::default();
        let mut quantizer = BinaryQuantizer::new(config);
        
        let vec1 = vec![1.0, -1.0, 1.0, -1.0];
        let vec2 = vec![1.0, 1.0, -1.0, -1.0];
        
        let bin1 = quantizer.quantize(&vec1).unwrap();
        let bin2 = quantizer.quantize(&vec2).unwrap();
        
        let distance = quantizer.hamming_distance(&bin1, &bin2).unwrap();
        assert!(distance > 0.0); // Should have some difference
    }
    
    #[test]
    fn test_binary_vector_store() {
        let config = BinaryQuantizationConfig::default();
        let mut store = BinaryVectorStore::new(config);
        
        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        let vector = vec![0.1, 0.2, 0.3];
        let binary_vec = quantizer.quantize(&vector).unwrap();
        
        store.add_vector(binary_vec, "test_id".to_string()).unwrap();
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
    }
} 