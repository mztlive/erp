//! BPM 图规则：线性连线生成与单条连线/入口校验。

pub mod linear;
pub mod validator;

pub use linear::{generate_linear_transitions, LinearTransitionDraft};
pub use validator::{validate_entry_node, validate_transition};
