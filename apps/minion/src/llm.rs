use std::sync::Arc;

use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartText,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
    CreateChatCompletionRequest, ImageDetail, ImageUrl,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::webp::WebPEncoder;
use image::{ColorType, ImageEncoder};
use thiserror::Error;

use crate::enclose;
use crate::util::retry_exp;

#[derive(Clone)]
pub struct LLMClient {
    client: Arc<async_openai::Client<OpenAIConfig>>,
}

#[derive(Error, Debug)]
#[error("Failed to prompt model")]
pub enum PromptError {
    OpenAI(#[from] async_openai::error::OpenAIError),
    #[error("Missing completion from response")]
    MissingCompletion,
}

impl LLMClient {
    pub fn new(base_url: &str, openai_key: &str) -> Self {
        let config = OpenAIConfig::new().with_api_base(base_url).with_api_key(openai_key);
        let client = Arc::new(async_openai::Client::with_config(config));
        Self { client }
    }

    pub async fn prompt(&self, model: &str, prompt: &Prompt) -> Result<String, PromptError> {
        let ctx = RenderCtx { model: model.to_owned() };
        let messages: Vec<ChatCompletionRequestMessage> = prompt.render(&ctx);
        let temperature = if ["o1-mini", "o1-preview"].contains(&model) { None } else { Some(0.0) };

        let request = CreateChatCompletionRequest {
            model: model.to_owned(),
            messages,
            temperature,
            stop: None,
            ..Default::default()
        };

        let client = self.client.clone();

        let response = retry_exp(move || {
            enclose! {
                (client, request)
                async move { Ok(client.chat().create(request).await?) }
            }
        })
        .await?;

        let completion =
            response.choices[0].message.content.clone().ok_or(PromptError::MissingCompletion)?;

        Ok(completion)
    }
}

pub struct RenderCtx {
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub items: Vec<PromptItem>,
}

impl Prompt {
    fn render(&self, ctx: &RenderCtx) -> Vec<ChatCompletionRequestMessage> {
        self.items.iter().map(|item| item.render(ctx)).collect()
    }
}

impl From<Vec<PromptItem>> for Prompt {
    fn from(items: Vec<PromptItem>) -> Self {
        Self { items }
    }
}

#[derive(Clone, Debug)]
pub enum PromptItem {
    User { content: Content },
    System { text: String },
    Assistant { text: String },
}

impl PromptItem {
    fn render(&self, ctx: &RenderCtx) -> ChatCompletionRequestMessage {
        match self {
            PromptItem::User { content } => {
                ChatCompletionRequestUserMessage { content: content.render(), ..Default::default() }
                    .into()
            }
            PromptItem::System { text } => {
                if ["o1-mini", "o1-preview"].contains(&ctx.model.as_str()) {
                    ChatCompletionRequestMessage::from(ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(text.to_owned()),
                        ..Default::default()
                    })
                } else {
                    ChatCompletionRequestSystemMessage {
                        content: text.clone().into(),
                        ..Default::default()
                    }
                    .into()
                }
            }
            PromptItem::Assistant { text } => ChatCompletionRequestAssistantMessage {
                content: Some(text.clone().into()),
                ..Default::default()
            }
            .into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Content {
    pub items: Vec<ContentItem>,
}

impl Content {
    fn render(&self) -> ChatCompletionRequestUserMessageContent {
        self.items.iter().map(ContentItem::render).collect::<Vec<_>>().into()
    }
}

impl From<Vec<ContentItem>> for Content {
    fn from(items: Vec<ContentItem>) -> Self {
        Self { items }
    }
}

impl From<String> for Content {
    fn from(text: String) -> Self {
        Self { items: vec![ContentItem::Text { text }] }
    }
}

#[derive(Clone, Debug)]
pub enum ContentItem {
    Text {
        text: String,
    },
    #[allow(dead_code)]
    Image {
        image_base64_webp: String,
    },
}

impl ContentItem {
    #[allow(dead_code)]
    pub fn from_rgba_image(image: image::RgbaImage) -> Self {
        let mut image_webp = Vec::new();
        WebPEncoder::new_lossless(&mut image_webp)
            .write_image(image.as_raw(), image.width(), image.height(), ColorType::Rgba8)
            .unwrap();

        let image_base64_webp = STANDARD.encode(image_webp);
        Self::Image { image_base64_webp }
    }

    fn render(&self) -> ChatCompletionRequestUserMessageContentPart {
        match self {
            ContentItem::Text { text } => {
                ChatCompletionRequestMessageContentPartText { text: text.to_owned() }.into()
            }
            ContentItem::Image { image_base64_webp } => {
                let url = format!("data:image/webp;base64,{}", image_base64_webp);
                ChatCompletionRequestMessageContentPartImage {
                    image_url: ImageUrl { url, detail: Some(ImageDetail::High) },
                }
                .into()
            }
        }
    }
}
