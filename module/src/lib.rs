use spacetimedb::{reducer, table, view, Query, ReducerContext, Table, ViewContext};

#[table(name = messages, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub content: String,
}

#[view(name = messages_view, public)]
pub fn messages_view(ctx: &ViewContext) -> Query<Message> {
    ctx.from.messages().build()
}

#[reducer]
pub fn append_message(ctx: &ReducerContext, content: String) {
    ctx.db.messages().insert(Message { id: 0, content });
}
