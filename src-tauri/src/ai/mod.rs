//! AI integration: scene generation from natural-language prompts.
//!
//! This module is intentionally a POC. The shape is:
//!   - [`config`] persists provider choice + API keys to the OS app
//!     config dir, off the show file. Show files get shared/synced;
//!     keys do not.
//!   - [`anthropic`] and [`openai`] each implement a single
//!     `generate_scene` entry point that takes the structured show
//!     context + user prompt and returns a [`scene_gen::DraftScene`].
//!     They use tool/function calling to force structured output —
//!     plain JSON-mode is too easy to derail with prose.
//!   - [`scene_gen`] is the orchestrator: it builds the context the
//!     LLM sees, dispatches to the configured provider, validates the
//!     response against the live show (no invented fixture IDs, no
//!     out-of-range channel offsets), and returns a draft the
//!     frontend previews before applying.
//!
//! Why two providers in parallel: the operator wants to compare
//! model fidelity for lighting cues, and the cost/latency profiles
//! differ enough that having both available matters during the POC.

pub mod anthropic;
pub mod config;
pub mod openai;
pub mod scene_gen;
