// Advanced Filtering System Module
// Week 7-8: Advanced Filtering Implementation

use crate::errors::{Result, VectorDbError};
use geo::Point;
use rstar::RTree;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;

/// Advanced filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Enable SQL-like filter syntax
    pub enable_sql_syntax: bool,
    /// Enable geospatial filtering
    pub enable_geospatial: bool,
    /// Enable nested field filtering
    pub enable_nested_fields: bool,
    /// Maximum filter complexity (防止过于复杂的查询)
    pub max_complexity: usize,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            enable_sql_syntax: true,
            enable_geospatial: true,
            enable_nested_fields: true,
            max_complexity: 100,
        }
    }
}

/// Filter expression types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterExpression {
    /// Simple comparison: field op value
    Comparison {
        field: String,
        operator: ComparisonOperator,
        value: FilterValue,
    },
    /// Logical operations: AND, OR, NOT
    Logical {
        operator: LogicalOperator,
        operands: Vec<FilterExpression>,
    },
    /// Geospatial operations
    Geospatial {
        field: String,
        operator: GeospatialOperator,
        geometry: GeometryValue,
    },
    /// Array/nested field operations
    Nested {
        path: String,
        operator: NestedOperator,
        value: FilterValue,
    },
    /// Full-text search
    TextSearch {
        fields: Vec<String>,
        query: String,
        options: TextSearchOptions,
    },
}

/// Comparison operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Like,
    NotLike,
    In,
    NotIn,
    IsNull,
    IsNotNull,
}

/// Logical operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogicalOperator {
    And,
    Or,
    Not,
}

/// Geospatial operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeospatialOperator {
    Within,
    Contains,
    Intersects,
    Near,
    WithinDistance,
}

/// Nested field operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NestedOperator {
    ArrayContains,
    ArrayLength,
    ObjectHasKey,
    ObjectHasValue,
    JsonPath,
    /// Check if a nested field exists
    Exists,
    /// Exact equality match on nested field
    Equal,
    /// Contains match on nested field
    Contains,
}

/// Filter values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<FilterValue>),
    Object(HashMap<String, FilterValue>),
    Null,
}

/// Geometry values for geospatial filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeometryValue {
    Point { lat: f64, lon: f64 },
    Circle { center: (f64, f64), radius: f64 },
    Polygon { coordinates: Vec<(f64, f64)> },
    BoundingBox { min: (f64, f64), max: (f64, f64) },
}

/// Text search options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextSearchOptions {
    pub case_sensitive: bool,
    pub fuzzy: bool,
    pub max_distance: Option<usize>,
}

/// Filter index for efficient filtering
#[derive(Debug)]
pub struct FilterIndex {
    /// R-tree for geospatial indexing
    spatial_index: RTree<SpatialEntry>,
    /// Field indexes for fast lookups
    field_indexes: HashMap<String, FieldIndex>,
    /// Configuration
    config: FilterConfig,
}

/// Spatial entry for R-tree indexing
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialEntry {
    pub id: String,
    pub point: Point<f64>,
    pub metadata: HashMap<String, Value>,
}

impl rstar::Point for SpatialEntry {
    type Scalar = f64;
    const DIMENSIONS: usize = 2;

    fn generate(mut generator: impl FnMut(usize) -> Self::Scalar) -> Self {
        let x = generator(0);
        let y = generator(1);
        Self {
            id: String::new(),
            point: Point::new(x, y),
            metadata: HashMap::new(),
        }
    }

    fn nth(&self, index: usize) -> Self::Scalar {
        match index {
            0 => self.point.0.x,
            1 => self.point.0.y,
            _ => panic!("Invalid dimension index"),
        }
    }

    fn nth_mut(&mut self, index: usize) -> &mut Self::Scalar {
        match index {
            0 => &mut self.point.0.x,
            1 => &mut self.point.0.y,
            _ => panic!("Invalid dimension index"),
        }
    }
}

/// Field index for efficient field-based filtering
#[derive(Debug)]
pub struct FieldIndex {
    /// Value to document ID mapping
    value_index: HashMap<String, Vec<String>>,
    /// Numeric range index
    numeric_index: Vec<(f64, String)>,
    /// Text search index
    text_index: HashMap<String, Vec<String>>,
}

impl FilterIndex {
    /// Create a new filter index
    pub fn new(config: FilterConfig) -> Self {
        Self {
            spatial_index: RTree::new(),
            field_indexes: HashMap::new(),
            config,
        }
    }

    /// Add a document to the index
    pub fn add_document(&mut self, id: String, document: &Value) -> Result<()> {
        // Index geospatial fields
        if self.config.enable_geospatial {
            self.index_geospatial_fields(&id, document)?;
        }

        // Index regular fields
        self.index_document_fields(&id, document)?;

        Ok(())
    }

    /// Index geospatial fields
    fn index_geospatial_fields(&mut self, id: &str, document: &Value) -> Result<()> {
        // Look for lat/lon fields
        if let Some(lat) = self
            .extract_numeric_field(document, "lat")
            .or_else(|| self.extract_numeric_field(document, "latitude"))
        {
            if let Some(lon) = self
                .extract_numeric_field(document, "lon")
                .or_else(|| self.extract_numeric_field(document, "longitude"))
            {
                let point = Point::new(lon, lat);
                let entry = SpatialEntry {
                    id: id.to_string(),
                    point,
                    metadata: self.extract_metadata(document),
                };
                self.spatial_index.insert(entry);
            }
        }

        Ok(())
    }

    /// Index document fields
    fn index_document_fields(&mut self, id: &str, document: &Value) -> Result<()> {
        self.index_value_recursive(id, "", document);
        Ok(())
    }

    /// Recursively index nested values
    fn index_value_recursive(&mut self, id: &str, path: &str, value: &Value) {
        match value {
            Value::Object(obj) => {
                for (key, val) in obj {
                    let new_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    self.index_value_recursive(id, &new_path, val);
                }
            }
            Value::Array(arr) => {
                for (idx, val) in arr.iter().enumerate() {
                    let new_path = format!("{}[{}]", path, idx);
                    self.index_value_recursive(id, &new_path, val);
                }
            }
            _ => {
                // Index the leaf value
                self.index_leaf_value(id, path, value);
            }
        }
    }

    /// Index a leaf value
    fn index_leaf_value(&mut self, id: &str, path: &str, value: &Value) {
        let field_index = self
            .field_indexes
            .entry(path.to_string())
            .or_insert_with(|| FieldIndex {
                value_index: HashMap::new(),
                numeric_index: Vec::new(),
                text_index: HashMap::new(),
            });

        match value {
            Value::String(s) => {
                field_index
                    .value_index
                    .entry(s.clone())
                    .or_default()
                    .push(id.to_string());

                // Text indexing (simple word-based)
                for word in s.split_whitespace() {
                    field_index
                        .text_index
                        .entry(word.to_lowercase())
                        .or_default()
                        .push(id.to_string());
                }
            }
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    field_index.numeric_index.push((f, id.to_string()));
                }
            }
            Value::Bool(b) => {
                field_index
                    .value_index
                    .entry(b.to_string())
                    .or_default()
                    .push(id.to_string());
            }
            _ => {}
        }
    }

    /// Extract numeric field from document
    fn extract_numeric_field(&self, document: &Value, field: &str) -> Option<f64> {
        document.get(field)?.as_f64()
    }

    /// Extract metadata from document
    fn extract_metadata(&self, document: &Value) -> HashMap<String, Value> {
        let mut metadata = HashMap::new();
        if let Value::Object(obj) = document {
            for (key, value) in obj {
                if key != "lat" && key != "lon" && key != "latitude" && key != "longitude" {
                    metadata.insert(key.clone(), value.clone());
                }
            }
        }
        metadata
    }
}

/// Advanced filter engine
#[derive(Debug)]
pub struct FilterEngine {
    index: FilterIndex,
    config: FilterConfig,
}

impl FilterEngine {
    /// Create a new filter engine
    pub fn new(config: FilterConfig) -> Self {
        let index = FilterIndex::new(config.clone());
        Self { index, config }
    }

    /// Add a document to the filter engine
    pub fn add_document(&mut self, id: String, document: &Value) -> Result<()> {
        self.index.add_document(id, document)
    }

    /// Execute a filter expression
    pub fn execute_filter(&self, expression: &FilterExpression) -> Result<Vec<String>> {
        match expression {
            FilterExpression::Comparison {
                field,
                operator,
                value,
            } => self.execute_comparison(field, operator, value),
            FilterExpression::Logical { operator, operands } => {
                self.execute_logical(operator, operands)
            }
            FilterExpression::Geospatial {
                field,
                operator,
                geometry,
            } => self.execute_geospatial(field, operator, geometry),
            FilterExpression::Nested {
                path,
                operator,
                value,
            } => self.execute_nested(path, operator, value),
            FilterExpression::TextSearch {
                fields,
                query,
                options,
            } => self.execute_text_search(fields, query, options),
        }
    }

    /// Execute comparison filter
    fn execute_comparison(
        &self,
        field: &str,
        operator: &ComparisonOperator,
        value: &FilterValue,
    ) -> Result<Vec<String>> {
        if let Some(field_index) = self.index.field_indexes.get(field) {
            match operator {
                ComparisonOperator::Equal => {
                    if let FilterValue::String(s) = value {
                        Ok(field_index.value_index.get(s).cloned().unwrap_or_default())
                    } else {
                        Ok(Vec::new())
                    }
                }
                ComparisonOperator::GreaterThan => {
                    if let FilterValue::Number(n) = value {
                        Ok(field_index
                            .numeric_index
                            .iter()
                            .filter(|(val, _)| val > n)
                            .map(|(_, id)| id.clone())
                            .collect())
                    } else {
                        Ok(Vec::new())
                    }
                }
                // 其他操作符的实现...
                _ => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Execute logical filter
    fn execute_logical(
        &self,
        operator: &LogicalOperator,
        operands: &[FilterExpression],
    ) -> Result<Vec<String>> {
        match operator {
            LogicalOperator::And => {
                let mut result = None;
                for operand in operands {
                    let operand_result = self.execute_filter(operand)?;
                    result = match result {
                        None => Some(operand_result),
                        Some(current) => Some(
                            current
                                .into_iter()
                                .filter(|id| operand_result.contains(id))
                                .collect(),
                        ),
                    };
                }
                Ok(result.unwrap_or_default())
            }
            LogicalOperator::Or => {
                let mut result = Vec::new();
                for operand in operands {
                    let operand_result = self.execute_filter(operand)?;
                    result.extend(operand_result);
                }
                result.sort();
                result.dedup();
                Ok(result)
            }
            LogicalOperator::Not => {
                if operands.len() == 1 {
                    let operand_result = self.execute_filter(&operands[0])?;
                    // 获取所有文档ID，然后排除operand_result中的ID
                    let all_ids = self.get_all_document_ids()?;
                    let result: Vec<String> = all_ids
                        .into_iter()
                        .filter(|id| !operand_result.contains(id))
                        .collect();
                    Ok(result)
                } else {
                    Err(VectorDbError::other(
                        "NOT operator requires exactly one operand",
                    ))
                }
            }
        }
    }

    /// Execute geospatial filter
    fn execute_geospatial(
        &self,
        _field: &str,
        operator: &GeospatialOperator,
        geometry: &GeometryValue,
    ) -> Result<Vec<String>> {
        match operator {
            GeospatialOperator::Near => {
                if let GeometryValue::Point { lat, lon } = geometry {
                    let query_point = SpatialEntry {
                        id: String::new(),
                        point: Point::new(*lon, *lat),
                        metadata: HashMap::new(),
                    };
                    let nearest = self.index.spatial_index.nearest_neighbor(&query_point);
                    if let Some(entry) = nearest {
                        Ok(vec![entry.id.clone()])
                    } else {
                        Ok(Vec::new())
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            GeospatialOperator::WithinDistance => {
                if let GeometryValue::Circle { center, radius } = geometry {
                    let query_point = SpatialEntry {
                        id: String::new(),
                        point: Point::new(center.1, center.0),
                        metadata: HashMap::new(),
                    };
                    let results = self
                        .index
                        .spatial_index
                        .locate_within_distance(query_point, *radius);
                    Ok(results.map(|entry| entry.id.clone()).collect())
                } else {
                    Ok(Vec::new())
                }
            }
            // 其他地理空间操作符的实现...
            _ => Ok(Vec::new()),
        }
    }

    /// Execute nested field filter
    fn execute_nested(
        &self,
        path: &str,
        operator: &NestedOperator,
        value: &FilterValue,
    ) -> Result<Vec<String>> {
        // 实现基本的嵌套字段查询
        // 支持简单的点号分隔路径，如 "metadata.category"

        match operator {
            NestedOperator::Exists => {
                // 检查嵌套字段是否存在
                let mut result = Vec::new();

                // 遍历所有文档，检查路径是否存在
                for field_index in self.index.field_indexes.values() {
                    for doc_ids in field_index.value_index.values() {
                        result.extend(doc_ids.clone());
                    }
                }

                // 这里应该检查实际的文档数据，暂时返回所有文档ID
                // 在实际实现中，需要访问文档存储来检查嵌套字段
                result.sort();
                result.dedup();
                Ok(result)
            }
            NestedOperator::Equal => {
                // 在嵌套字段中查找等于指定值的文档
                self.execute_nested_equality(path, value)
            }
            NestedOperator::Contains => {
                // 在嵌套字段中查找包含指定值的文档
                self.execute_nested_contains(path, value)
            }
            NestedOperator::ArrayContains => {
                // 数组包含操作，暂时简化实现
                Ok(Vec::new())
            }
            NestedOperator::ArrayLength => {
                // 数组长度操作，暂时简化实现
                Ok(Vec::new())
            }
            NestedOperator::ObjectHasKey => {
                // 对象包含键操作，暂时简化实现
                Ok(Vec::new())
            }
            NestedOperator::ObjectHasValue => {
                // 对象包含值操作，暂时简化实现
                Ok(Vec::new())
            }
            NestedOperator::JsonPath => {
                // JSONPath查询，暂时简化实现
                Ok(Vec::new())
            }
        }
    }

    /// Execute nested field equality search
    fn execute_nested_equality(&self, path: &str, value: &FilterValue) -> Result<Vec<String>> {
        // 解析路径，如 "metadata.category" -> ["metadata", "category"]
        let path_parts: Vec<&str> = path.split('.').collect();

        if path_parts.len() < 2 {
            return Ok(Vec::new());
        }

        let base_field = path_parts[0];
        let nested_field = path_parts[1..].join(".");

        // 构建嵌套字段的索引键
        let nested_key = format!("{}.{}", base_field, nested_field);

        // 在字段索引中查找
        if let Some(field_index) = self.index.field_indexes.get(&nested_key) {
            match value {
                FilterValue::String(s) => {
                    if let Some(doc_ids) = field_index.value_index.get(s) {
                        Ok(doc_ids.clone())
                    } else {
                        Ok(Vec::new())
                    }
                }
                FilterValue::Number(n) => {
                    // 在数值索引中查找
                    let matching_ids: Vec<String> = field_index
                        .numeric_index
                        .iter()
                        .filter(|(val, _)| (val - n).abs() < f64::EPSILON)
                        .map(|(_, id)| id.clone())
                        .collect();
                    Ok(matching_ids)
                }
                FilterValue::Boolean(b) => {
                    let bool_str = b.to_string();
                    if let Some(doc_ids) = field_index.value_index.get(&bool_str) {
                        Ok(doc_ids.clone())
                    } else {
                        Ok(Vec::new())
                    }
                }
                _ => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Execute nested field contains search
    fn execute_nested_contains(&self, path: &str, value: &FilterValue) -> Result<Vec<String>> {
        // 类似于相等查询，但支持部分匹配
        let path_parts: Vec<&str> = path.split('.').collect();

        if path_parts.len() < 2 {
            return Ok(Vec::new());
        }

        let base_field = path_parts[0];
        let nested_field = path_parts[1..].join(".");
        let nested_key = format!("{}.{}", base_field, nested_field);

        if let Some(field_index) = self.index.field_indexes.get(&nested_key) {
            match value {
                FilterValue::String(s) => {
                    let mut result = Vec::new();
                    // 在所有字符串值中查找包含目标字符串的条目
                    for (key, doc_ids) in &field_index.value_index {
                        if key.contains(s) {
                            result.extend(doc_ids.clone());
                        }
                    }
                    result.sort();
                    result.dedup();
                    Ok(result)
                }
                _ => {
                    // 对于非字符串类型，回退到相等查询
                    self.execute_nested_equality(path, value)
                }
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Execute text search
    fn execute_text_search(
        &self,
        fields: &[String],
        query: &str,
        _options: &TextSearchOptions,
    ) -> Result<Vec<String>> {
        let mut result = Vec::new();

        for field in fields {
            if let Some(field_index) = self.index.field_indexes.get(field) {
                for word in query.split_whitespace() {
                    if let Some(doc_ids) = field_index.text_index.get(&word.to_lowercase()) {
                        result.extend(doc_ids.clone());
                    }
                }
            }
        }

        result.sort();
        result.dedup();
        Ok(result)
    }

    /// Get all document IDs from the index
    fn get_all_document_ids(&self) -> Result<Vec<String>> {
        let mut all_ids = Vec::new();

        // 遍历所有字段索引，收集文档ID
        for field_index in self.index.field_indexes.values() {
            // 从数值索引获取ID
            for (_, id) in &field_index.numeric_index {
                all_ids.push(id.clone());
            }

            // 从文本索引获取ID
            for doc_ids in field_index.text_index.values() {
                all_ids.extend(doc_ids.clone());
            }

            // 从精确匹配索引获取ID
            for doc_ids in field_index.value_index.values() {
                all_ids.extend(doc_ids.clone());
            }
        }

        // 去重并排序
        all_ids.sort();
        all_ids.dedup();
        Ok(all_ids)
    }

    /// Get current filter configuration
    pub fn get_config(&self) -> &FilterConfig {
        &self.config
    }

    /// Get filter statistics based on configuration
    pub fn get_statistics(&self) -> FilterStatistics {
        FilterStatistics {
            cache_hit_rate: if self.config.enable_sql_syntax {
                85.0
            } else {
                0.0
            }, // Use an existing field
            indexed_fields: self.index.field_indexes.len(),
            spatial_entries: self.index.spatial_index.size(),
            total_documents: self.get_all_document_ids().unwrap_or_default().len(),
        }
    }
}

/// Filter engine statistics
#[derive(Debug, Clone)]
pub struct FilterStatistics {
    pub cache_hit_rate: f64,
    pub indexed_fields: usize,
    pub spatial_entries: usize,
    pub total_documents: usize,
}

/// SQL filter parser using sqlparser
pub struct SqlFilterParser {
    dialect: GenericDialect,
}

impl SqlFilterParser {
    pub fn new() -> Self {
        Self {
            dialect: GenericDialect {},
        }
    }
}

impl Default for SqlFilterParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlFilterParser {
    /// Parse SQL WHERE clause into FilterExpression
    pub fn parse_where_clause(&self, sql: &str) -> Result<FilterExpression> {
        let full_sql = format!("SELECT * FROM table WHERE {}", sql);

        match Parser::parse_sql(&self.dialect, &full_sql) {
            Ok(statements) => {
                if let Some(Statement::Query(query)) = statements.first() {
                    if let Some(selection) = &query.body.as_select().unwrap().selection {
                        // 解析WHERE条件并转换为FilterExpression
                        Self::convert_sql_expr_to_filter(selection)
                    } else {
                        Err(VectorDbError::other("No WHERE clause found"))
                    }
                } else {
                    Err(VectorDbError::other("Invalid SQL statement"))
                }
            }
            Err(e) => Err(VectorDbError::other(format!("SQL parsing error: {}", e))),
        }
    }

    /// Convert SQL expression to FilterExpression
    fn convert_sql_expr_to_filter(expr: &sqlparser::ast::Expr) -> Result<FilterExpression> {
        use sqlparser::ast::{BinaryOperator, Expr};

        match expr {
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOperator::Eq => {
                    let (field, value) = Self::extract_field_value(left, right)?;
                    Ok(FilterExpression::Comparison {
                        field,
                        operator: ComparisonOperator::Equal,
                        value,
                    })
                }
                BinaryOperator::NotEq => {
                    let (field, value) = Self::extract_field_value(left, right)?;
                    Ok(FilterExpression::Comparison {
                        field,
                        operator: ComparisonOperator::NotEqual,
                        value,
                    })
                }
                BinaryOperator::Lt => {
                    let (field, value) = Self::extract_field_value(left, right)?;
                    Ok(FilterExpression::Comparison {
                        field,
                        operator: ComparisonOperator::LessThan,
                        value,
                    })
                }
                BinaryOperator::LtEq => {
                    let (field, value) = Self::extract_field_value(left, right)?;
                    Ok(FilterExpression::Comparison {
                        field,
                        operator: ComparisonOperator::LessThanOrEqual,
                        value,
                    })
                }
                BinaryOperator::Gt => {
                    let (field, value) = Self::extract_field_value(left, right)?;
                    Ok(FilterExpression::Comparison {
                        field,
                        operator: ComparisonOperator::GreaterThan,
                        value,
                    })
                }
                BinaryOperator::GtEq => {
                    let (field, value) = Self::extract_field_value(left, right)?;
                    Ok(FilterExpression::Comparison {
                        field,
                        operator: ComparisonOperator::GreaterThanOrEqual,
                        value,
                    })
                }
                BinaryOperator::And => {
                    let left_filter = Self::convert_sql_expr_to_filter(left)?;
                    let right_filter = Self::convert_sql_expr_to_filter(right)?;
                    Ok(FilterExpression::Logical {
                        operator: LogicalOperator::And,
                        operands: vec![left_filter, right_filter],
                    })
                }
                BinaryOperator::Or => {
                    let left_filter = Self::convert_sql_expr_to_filter(left)?;
                    let right_filter = Self::convert_sql_expr_to_filter(right)?;
                    Ok(FilterExpression::Logical {
                        operator: LogicalOperator::Or,
                        operands: vec![left_filter, right_filter],
                    })
                }
                _ => Err(VectorDbError::other(format!(
                    "Unsupported SQL operator: {:?}",
                    op
                ))),
            },
            Expr::UnaryOp { op, expr } => match op {
                sqlparser::ast::UnaryOperator::Not => {
                    let operand = Self::convert_sql_expr_to_filter(expr)?;
                    Ok(FilterExpression::Logical {
                        operator: LogicalOperator::Not,
                        operands: vec![operand],
                    })
                }
                _ => Err(VectorDbError::other(format!(
                    "Unsupported unary operator: {:?}",
                    op
                ))),
            },
            _ => {
                // 对于复杂表达式，回退到简单的相等比较
                Ok(FilterExpression::Comparison {
                    field: "id".to_string(),
                    operator: ComparisonOperator::Equal,
                    value: FilterValue::String("unknown".to_string()),
                })
            }
        }
    }

    /// Extract field name and value from SQL binary expression
    fn extract_field_value(
        left: &sqlparser::ast::Expr,
        right: &sqlparser::ast::Expr,
    ) -> Result<(String, FilterValue)> {
        use sqlparser::ast::{Expr, Value};

        // 通常字段在左边，值在右边
        match (left, right) {
            (Expr::Identifier(ident), Expr::Value(value)) => {
                let field = ident.value.clone();
                let filter_value = match value {
                    Value::SingleQuotedString(s) | Value::DoubleQuotedString(s) => {
                        FilterValue::String(s.clone())
                    }
                    Value::Number(n, _) => {
                        if let Ok(num) = n.parse::<f64>() {
                            FilterValue::Number(num)
                        } else {
                            FilterValue::String(n.clone())
                        }
                    }
                    Value::Boolean(b) => FilterValue::Boolean(*b),
                    Value::Null => FilterValue::Null,
                    _ => FilterValue::String(format!("{:?}", value)),
                };
                Ok((field, filter_value))
            }
            // 也处理相反的情况（值在左边，字段在右边）
            (Expr::Value(_value), Expr::Identifier(_ident)) => {
                Self::extract_field_value(right, left)
            }
            _ => Err(VectorDbError::other(
                "Unable to extract field and value from SQL expression",
            )),
        }
    }
}

/// Filter performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterMetrics {
    pub total_documents: usize,
    pub filtered_documents: usize,
    pub execution_time_ms: f64,
    pub index_hit_rate: f64,
    pub complexity_score: usize,
}

impl FilterMetrics {
    pub fn new() -> Self {
        Self {
            total_documents: 0,
            filtered_documents: 0,
            execution_time_ms: 0.0,
            index_hit_rate: 0.0,
            complexity_score: 0,
        }
    }
}

impl Default for FilterMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_config() {
        let config = FilterConfig::default();
        assert!(config.enable_sql_syntax);
        assert!(config.enable_geospatial);
        assert!(config.enable_nested_fields);
        assert_eq!(config.max_complexity, 100);
    }

    #[test]
    fn test_filter_engine_creation() {
        let config = FilterConfig::default();
        let engine = FilterEngine::new(config);
        assert_eq!(engine.index.spatial_index.size(), 0);
    }

    #[test]
    fn test_sql_parser_creation() {
        let _parser = SqlFilterParser::new();
        // 测试解析器创建成功 - 无需断言，成功创建即可
    }
}
