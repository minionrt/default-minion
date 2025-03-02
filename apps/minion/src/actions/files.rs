use crate::container::{Container, ReadFileError};

use super::markdown::strip_wrapping_markdown_code_fences;

pub async fn read_file(container: &Container, filename: &str) -> Result<String, ReadFileError> {
    container.read_file(filename).await
}

pub async fn write_file(container: &Container, filename: &str, content: &str) {
    let content = strip_wrapping_markdown_code_fences(content);
    container.write_file(filename, &content).await.unwrap()
}
