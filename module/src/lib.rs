use spacetimedb::{reducer, table, view, AnonymousViewContext, Query, ReducerContext, Table};

#[table(accessor = messages, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub content: String,
}

#[view(accessor = messages_view, public)]
pub fn messages_view(ctx: &AnonymousViewContext) -> impl Query<Message> {
    ctx.from.messages()
}

#[reducer]
pub fn append_message(ctx: &ReducerContext, content: String) {
    ctx.db.messages().insert(Message { id: 0, content });
}
