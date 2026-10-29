//! `lazyos mcp`: the desktop as an MCP server (spec 008).
//!
//! The agent's hands on the desktop, with the plan's contract built into the
//! types: the AI **reads**, **acts** visibly and reversibly, or **proposes** by
//! opening the real gated window - and the person confirms there. What would
//! apply a destructive change does not exist as a tool at all.
//!
//! - [`manifest`] is the table (`ToolSpec`): names, tiers, schemas, the program
//!   each tool runs, the limits. `tools/list`, `--tools` and the manifest
//!   version are generated from it, and a `const` assertion keeps a refused
//!   name out of it.
//! - [`schema`] validates a call's arguments before anything runs.
//! - [`server`] is JSON-RPC 2.0 over stdio: the framing, the methods, the call
//!   log, the limits. The effects are a [`server::Handler`] the binary
//!   implements with the programs the menus run, so there is one
//!   implementation per action.

pub mod manifest;
pub mod schema;
pub mod server;

pub use manifest::{Limit, Maps, REFUSED, TOOLS, Tier, ToolSpec};
pub use server::{CallLog, Handler, Reply, Resource, Server, serve};
