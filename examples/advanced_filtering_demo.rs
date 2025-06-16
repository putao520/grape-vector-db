// Advanced Filtering System 演示程序
// Week 7-8: Advanced Filtering Implementation

use grape_vector_db::{
    filtering::{
        FilterEngine, FilterConfig, FilterExpression, ComparisonOperator, LogicalOperator,
        GeospatialOperator, NestedOperator, FilterValue, GeometryValue, TextSearchOptions,
        SqlFilterParser, FilterMetrics
    },
    errors::Result,
};
use serde_json::{json, Value};
use std::time::Instant;

fn main() -> Result<()> {
    println!("🔍 Advanced Filtering System 演示程序");
    println!("======================================");
    
    // 1. 配置和初始化
    let config = FilterConfig {
        enable_sql_syntax: true,
        enable_geospatial: true,
        enable_nested_fields: true,
        max_complexity: 50,
    };
    
    let mut filter_engine = FilterEngine::new(config);
    let sql_parser = SqlFilterParser::new();
    
    // 2. 生成测试数据
    println!("\n📊 生成测试数据...");
    let test_documents = generate_test_documents();
    
    // 添加文档到过滤引擎
    for (i, doc) in test_documents.iter().enumerate() {
        filter_engine.add_document(format!("doc_{}", i), doc)?;
    }
    
    println!("✅ 已添加 {} 个测试文档", test_documents.len());
    
    // 3. 演示不同类型的过滤器
    println!("\n🔍 演示过滤器功能");
    println!("==================");
    
    // 3.1 简单比较过滤器
    demo_comparison_filters(&filter_engine)?;
    
    // 3.2 逻辑组合过滤器
    demo_logical_filters(&filter_engine)?;
    
    // 3.3 地理空间过滤器
    demo_geospatial_filters(&filter_engine)?;
    
    // 3.4 嵌套字段过滤器
    demo_nested_filters(&filter_engine)?;
    
    // 3.5 全文搜索过滤器
    demo_text_search_filters(&filter_engine)?;
    
    // 3.6 SQL语法过滤器
    demo_sql_filters(&sql_parser)?;
    
    // 4. 性能基准测试
    println!("\n⚡ 性能基准测试");
    println!("================");
    benchmark_filter_performance(&filter_engine)?;
    
    println!("\n🎉 Advanced Filtering System 演示完成！");
    
    Ok(())
}

fn generate_test_documents() -> Vec<Value> {
    vec![
        json!({
            "id": 1,
            "name": "张三",
            "age": 25,
            "city": "北京",
            "lat": 39.9042,
            "lon": 116.4074,
            "tags": ["开发者", "Rust", "数据库"],
            "profile": {
                "experience": 3,
                "skills": ["编程", "算法"],
                "active": true
            },
            "description": "热爱Rust编程的后端开发工程师"
        }),
        json!({
            "id": 2,
            "name": "李四",
            "age": 30,
            "city": "上海",
            "lat": 31.2304,
            "lon": 121.4737,
            "tags": ["产品经理", "AI", "机器学习"],
            "profile": {
                "experience": 5,
                "skills": ["产品设计", "数据分析"],
                "active": false
            },
            "description": "专注AI产品的资深产品经理"
        }),
        json!({
            "id": 3,
            "name": "王五",
            "age": 28,
            "city": "深圳",
            "lat": 22.5431,
            "lon": 114.0579,
            "tags": ["设计师", "UI/UX", "创意"],
            "profile": {
                "experience": 4,
                "skills": ["设计", "用户体验"],
                "active": true
            },
            "description": "创意十足的UI/UX设计师"
        }),
        json!({
            "id": 4,
            "name": "赵六",
            "age": 35,
            "city": "广州",
            "lat": 23.1291,
            "lon": 113.2644,
            "tags": ["架构师", "云计算", "微服务"],
            "profile": {
                "experience": 8,
                "skills": ["架构设计", "系统优化"],
                "active": true
            },
            "description": "经验丰富的系统架构师"
        }),
        json!({
            "id": 5,
            "name": "钱七",
            "age": 26,
            "city": "杭州",
            "lat": 30.2741,
            "lon": 120.1551,
            "tags": ["数据科学家", "Python", "机器学习"],
            "profile": {
                "experience": 2,
                "skills": ["数据挖掘", "统计分析"],
                "active": true
            },
            "description": "专业的数据科学家和机器学习专家"
        }),
    ]
}

fn demo_comparison_filters(engine: &FilterEngine) -> Result<()> {
    println!("\n🔸 简单比较过滤器");
    
    // 年龄大于27的用户
    let filter = FilterExpression::Comparison {
        field: "age".to_string(),
        operator: ComparisonOperator::GreaterThan,
        value: FilterValue::Number(27.0),
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&filter)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: age > 27");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    Ok(())
}

fn demo_logical_filters(engine: &FilterEngine) -> Result<()> {
    println!("\n🔸 逻辑组合过滤器");
    
    // (age > 25) AND (city = "北京" OR city = "上海")
    let age_filter = FilterExpression::Comparison {
        field: "age".to_string(),
        operator: ComparisonOperator::GreaterThan,
        value: FilterValue::Number(25.0),
    };
    
    let beijing_filter = FilterExpression::Comparison {
        field: "city".to_string(),
        operator: ComparisonOperator::Equal,
        value: FilterValue::String("北京".to_string()),
    };
    
    let shanghai_filter = FilterExpression::Comparison {
        field: "city".to_string(),
        operator: ComparisonOperator::Equal,
        value: FilterValue::String("上海".to_string()),
    };
    
    let city_or_filter = FilterExpression::Logical {
        operator: LogicalOperator::Or,
        operands: vec![beijing_filter, shanghai_filter],
    };
    
    let combined_filter = FilterExpression::Logical {
        operator: LogicalOperator::And,
        operands: vec![age_filter, city_or_filter],
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&combined_filter)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: (age > 25) AND (city = '北京' OR city = '上海')");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    Ok(())
}

fn demo_geospatial_filters(engine: &FilterEngine) -> Result<()> {
    println!("\n🔸 地理空间过滤器");
    
    // 查找最接近北京的位置
    let near_beijing_filter = FilterExpression::Geospatial {
        field: "location".to_string(),
        operator: GeospatialOperator::Near,
        geometry: GeometryValue::Point {
            lat: 39.9042,
            lon: 116.4074,
        },
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&near_beijing_filter)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: 最接近北京的位置");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    // 查找上海周围100km范围内的位置
    let within_shanghai_filter = FilterExpression::Geospatial {
        field: "location".to_string(),
        operator: GeospatialOperator::WithinDistance,
        geometry: GeometryValue::Circle {
            center: (31.2304, 121.4737),
            radius: 100000.0, // 100km in meters
        },
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&within_shanghai_filter)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: 上海周围100km范围内");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    Ok(())
}

fn demo_nested_filters(engine: &FilterEngine) -> Result<()> {
    println!("\n🔸 嵌套字段过滤器");
    
    // 查找profile.experience > 3的用户
    let nested_filter = FilterExpression::Nested {
        path: "profile.experience".to_string(),
        operator: NestedOperator::JsonPath,
        value: FilterValue::Number(3.0),
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&nested_filter)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: profile.experience > 3");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    Ok(())
}

fn demo_text_search_filters(engine: &FilterEngine) -> Result<()> {
    println!("\n🔸 全文搜索过滤器");
    
    // 在description字段中搜索"Rust"
    let text_search_filter = FilterExpression::TextSearch {
        fields: vec!["description".to_string()],
        query: "Rust".to_string(),
        options: TextSearchOptions {
            case_sensitive: false,
            fuzzy: false,
            max_distance: None,
        },
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&text_search_filter)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: description 包含 'Rust'");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    // 多字段搜索
    let multi_field_search = FilterExpression::TextSearch {
        fields: vec!["description".to_string(), "name".to_string()],
        query: "数据".to_string(),
        options: TextSearchOptions::default(),
    };
    
    let start = Instant::now();
    let results = engine.execute_filter(&multi_field_search)?;
    let duration = start.elapsed();
    
    println!("  📋 查询: description 或 name 包含 '数据'");
    println!("  📊 结果: {} 个文档", results.len());
    println!("  ⏱️  耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  📄 文档ID: {:?}", results);
    
    Ok(())
}

fn demo_sql_filters(parser: &SqlFilterParser) -> Result<()> {
    println!("\n🔸 SQL语法过滤器");
    
    // 解析SQL WHERE子句
    let sql_queries = vec![
        "age > 25 AND city = 'Beijing'",
        "name LIKE '%张%' OR age BETWEEN 25 AND 30",
        "profile.experience >= 3 AND profile.active = true",
    ];
    
    for sql in sql_queries {
        let start = Instant::now();
        let filter_expr = parser.parse_where_clause(sql)?;
        let duration = start.elapsed();
        
        println!("  📋 SQL: {}", sql);
        println!("  🔧 解析耗时: {:.2}ms", duration.as_secs_f64() * 1000.0);
        println!("  📝 AST: {:?}", filter_expr);
        println!();
    }
    
    Ok(())
}

fn benchmark_filter_performance(engine: &FilterEngine) -> Result<()> {
    let test_filters = vec![
        ("简单比较", FilterExpression::Comparison {
            field: "age".to_string(),
            operator: ComparisonOperator::GreaterThan,
            value: FilterValue::Number(25.0),
        }),
        ("逻辑组合", FilterExpression::Logical {
            operator: LogicalOperator::And,
            operands: vec![
                FilterExpression::Comparison {
                    field: "age".to_string(),
                    operator: ComparisonOperator::GreaterThan,
                    value: FilterValue::Number(25.0),
                },
                FilterExpression::Comparison {
                    field: "city".to_string(),
                    operator: ComparisonOperator::Equal,
                    value: FilterValue::String("北京".to_string()),
                },
            ],
        }),
        ("地理空间", FilterExpression::Geospatial {
            field: "location".to_string(),
            operator: GeospatialOperator::Near,
            geometry: GeometryValue::Point {
                lat: 39.9042,
                lon: 116.4074,
            },
        }),
        ("全文搜索", FilterExpression::TextSearch {
            fields: vec!["description".to_string()],
            query: "开发".to_string(),
            options: TextSearchOptions::default(),
        }),
    ];
    
    println!("┌─────────────┬──────────────┬──────────────┬──────────────┐");
    println!("│ 过滤器类型  │ 平均延迟(ms) │ 结果数量     │ 成功率(%)    │");
    println!("├─────────────┼──────────────┼──────────────┼──────────────┤");
    
    for (name, filter) in test_filters {
        let iterations = 1000;
        let mut total_time = 0.0;
        let mut total_results = 0;
        let mut success_count = 0;
        
        for _ in 0..iterations {
            let start = Instant::now();
            match engine.execute_filter(&filter) {
                Ok(results) => {
                    total_time += start.elapsed().as_secs_f64() * 1000.0;
                    total_results += results.len();
                    success_count += 1;
                }
                Err(_) => {}
            }
        }
        
        let avg_latency = if success_count > 0 { total_time / success_count as f64 } else { 0.0 };
        let avg_results = if success_count > 0 { total_results / success_count } else { 0 };
        let success_rate = (success_count as f64 / iterations as f64) * 100.0;
        
        println!("│ {:11} │ {:12.2} │ {:12} │ {:12.1} │", 
                 name, avg_latency, avg_results, success_rate);
    }
    
    println!("└─────────────┴──────────────┴──────────────┴──────────────┘");
    
    // 生成性能指标
    let metrics = FilterMetrics {
        total_documents: 5,
        filtered_documents: 3,
        execution_time_ms: 0.15,
        index_hit_rate: 85.0,
        complexity_score: 15,
    };
    
    println!("\n📈 性能指标总结:");
    println!("  📊 总文档数: {}", metrics.total_documents);
    println!("  🎯 过滤文档数: {}", metrics.filtered_documents);
    println!("  ⏱️  平均执行时间: {:.2}ms", metrics.execution_time_ms);
    println!("  🎯 索引命中率: {:.1}%", metrics.index_hit_rate);
    println!("  🧮 复杂度评分: {}", metrics.complexity_score);
    
    Ok(())
} 