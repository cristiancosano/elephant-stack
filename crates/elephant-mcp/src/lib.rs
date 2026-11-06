//! Fachada MCP por stdio (`elephant mcp`): un `ServerHandler` genérico que
//! recorre `elephant_api::catalog()`, sin `#[tool]` (05 §6.1). Es la única
//! crate que depende de rmcp y tokio.
//!
//! Esqueleto de P01; P09 lo implementa.
