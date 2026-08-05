# Entity-Base Crate

## 简介

`entity-core` crate 提供了项目中实体（Entity）的基础模型和接口，定义了各类实体的公共字段和行为。通过集成 `serde` 和 `chrono`，实现了实体的序列化、反序列化以及时间管理功能。

## 主要功能

- **基础模型**: 定义了如 `BaseModel` 等基础结构，包含诸如 ID、创建时间、更新时间等公共字段。
- **时间管理**: 集成 `chrono` 库，用于处理时间相关的操作。
- **序列化支持**: 利用 `serde` 实现实体的序列化和反序列化，便于数据的存储和传输。

## 安装与使用

### 环境要求

- Rust 1.56 及以上版本

### 使用方式

1. 在 `Cargo.toml` 中添加依赖：
    ```toml
    [dependencies]
    entity-core = { path = "../entity-core" }
    serde = { version = "1.0", features = ["derive"] }
    chrono = "0.4"
    ```

2. 使用基础模型：
    ```rust
    use entity_core::BaseModel;
    use serde::{Serialize, Deserialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct User {
        #[serde(flatten)]
        pub base: BaseModel,
        pub name: String,
        pub email: String,
    }

    impl User {
        pub fn new(id: String, name: String, email: String) -> Self {
            Self {
                base: BaseModel::new(id),
                name,
                email,
            }
        }
    }
    ```

## 贡献

欢迎提交问题和合并请求。请确保在提交之前运行所有测试并遵循项目的代码风格。

## 许可证

该项目使用 MIT 许可证。详情请参阅 LICENSE 文件。 