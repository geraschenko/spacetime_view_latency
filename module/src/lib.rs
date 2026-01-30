//! Minimal reproduction for view latency scaling issue.
//!
//! ## The Bug Hypothesis
//!
//! View subscriptions become slower as the total number of rows visible to a user grows.
//! Even fresh sessions experience slow inserts if the user has historical data in the database.
//!
//! ## Schema
//!
//! - `messages`: Main message table with sender identity (like chronicle_message)
//! - `messages_view`: View filtered by sender identity (like chronicle_message_view)
//!
//! ## Expected Behavior (if bug exists)
//!
//! Insert latency should grow linearly with the number of rows in the view.

use spacetimedb::{reducer, table, view, Identity, Query, ReducerContext, Table, Timestamp, ViewContext};

// ============================================================================
// Tables
// ============================================================================

/// Main message table (analogous to chronicle_message).
/// The sender field allows filtering by identity in the view.
#[table(name = messages, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub ts: Timestamp,
    /// The identity that sent this message
    #[index(btree)]
    pub sender: Identity,
    /// Message content
    pub content: String,
}

// ============================================================================
// Views
// ============================================================================

/// View that filters messages by sender identity.
/// Analogous to chronicle_message_view which uses a semijoin on message_visibility.
///
/// This is the simpler form - just filter by sender = ctx.sender.
#[view(name = messages_view, public)]
pub fn messages_view(ctx: &ViewContext) -> Query<Message> {
    ctx.from
        .messages()
        .r#where(|m| m.sender.eq(ctx.sender))
        .build()
}

// ============================================================================
// Reducers
// ============================================================================

/// Append a message.
/// The sender is automatically set to ctx.sender (the caller's identity).
#[reducer]
pub fn append_message(ctx: &ReducerContext, content: String) {
    ctx.db.messages().insert(Message {
        id: 0, // auto_inc
        ts: ctx.timestamp,
        sender: ctx.sender,
        content,
    });
}
