# Entity-Derive Crate

## 简介

`entity-macros` crate 是一个过程宏库，提供自定义的派生宏，用于简化实体（Entity）结构体的定义和实现。通过宏生成常用的实现代码，提高开发效率并减少重复代码。

## 主要功能

- **实体宏**: 提供 `Entity` 派生宏，自动生成实体相关的实现代码。
- **简化开发**: 减少手动编写重复的代码，提升代码的可维护性和可读性。
- **集成基础模型**: 与 `entity-core` crate 集成，扩展基础模型的功能。

## 安装与使用

### 环境要求

- Rust 1.56 及以上版本
- 使用 `proc-macro` 功能

### 使用方式

1. 在 `Cargo.toml` 中添加依赖：
    ```toml
    [dependencies]
    entity-macros = { path = "../entity-macros" }
    entity-core = { path = "../entity-core" }
    serde = { version = "1.0", features = ["derive"] }
    ```

2. 在实体定义中使用宏：
    ```rust
    use entity_macros::Entity;
    use entity_core::BaseModel;
    use serde::{Serialize, Deserialize};

    #[derive(Debug, Serialize, Deserialize, Entity)]
    pub struct Product {
        #[serde(flatten)]
        pub base: BaseModel,
        pub name: String,
        pub price: f64,
    }

    impl Product {
        pub fn new(id: String, name: String, price: f64) -> Self {
            Self {
                base: BaseModel::new(id),
                name,
                price,
            }
        }
    }
    ```

## 贡献

欢迎提交问题和合并请求。请确保在提交之前运行所有测试并遵循项目的代码风格。

## 许可证

该项目使用 MIT 许可证。详情请参阅 LICENSE 文件。 