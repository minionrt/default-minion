use crate::llm::{Prompt, PromptItem};

/// The maximum number of recent actions to keep in their entirety
const MAX_ACTIONS_TO_KEEP: usize = 5;

pub struct Action {
    pub number: usize,
    pub messages: Vec<PromptItem>,
    pub summary: String,
}

pub struct History {
    pub prefix: Vec<PromptItem>,
    pub actions: Vec<Action>,
}

impl History {
    pub fn new(prefix: Vec<PromptItem>) -> Self {
        Self { prefix, actions: Vec::new() }
    }

    /// Compresses the history by summarizing older actions and keeping only
    /// the last N actions in full.
    pub fn compressed_prompt(&self) -> Prompt {
        // Calculate how many actions need to be replaced by their summary
        let total_actions = self.actions.len();
        let skip_count = total_actions.saturating_sub(MAX_ACTIONS_TO_KEEP);

        let mut items = self.prefix.clone();

        // For the skipped (older) actions, store their summaries
        for action in &self.actions[..skip_count] {
            items.push(PromptItem::System {
                text: format!("Summary for action {}: {}", action.number, action.summary),
            });
        }

        // For the most recent actions, keep their messages in full
        for action in &self.actions[skip_count..] {
            items.extend(action.messages.clone());
        }

        Prompt { items }
    }

    /// Appends a new action to the history.
    pub fn append(&mut self, messages: Vec<PromptItem>, summary: String) {
        let number = self.actions.len();
        self.actions.push(Action { number, messages, summary });
    }
}
