use std::{fs, path::PathBuf};

use url::Url;

mod actions;
mod config;
mod container;
mod interaction_loop;
mod llm;
mod macros;
mod util;

#[tokio::main]
async fn main() {
    let config = config::Config::load();
    let api_url = config.api_base_url.unwrap();
    let api_token = config.api_token.unwrap();
    let agent_client = agent_api::Client::new(api_url.clone(), api_token.clone());
    let llm_client = llm::LLMClient::new(api_url.as_str(), &api_token);

    let task = agent_client.get_task().await.unwrap();

    let workspaces_dir = PathBuf::from("./workspaces");
    fs::create_dir(&workspaces_dir).unwrap();
    let workspace_dir_name = workspace_folder_name(&task.git_repo_url);
    let workspace_dir = workspaces_dir.join(&workspace_dir_name);

    let mut git_url = task.git_repo_url.clone();
    git_url.set_username("x-access-token").unwrap();
    git_url.set_password(Some(api_token.as_str())).unwrap();

    // Clone (and configure) the repository
    let git_repo = actions::git::Repo::clone(
        &workspace_dir,
        &git_url,
        &task.git_branch,
        &task.git_user_name,
        &task.git_user_email,
    );

    let container = container::Container::start(&workspace_dir, &workspace_dir_name).await;

    // Change the current directory to the project directory
    // The interaction loop will expect to be in the project directory
    std::env::set_current_dir(workspace_dir).expect("Failed to change current working directory");

    // Run the agent loop
    let outcome = interaction_loop::run(&llm_client, &container, &task).await;

    // Handle the outcome
    match outcome {
        interaction_loop::TaskOutcome::Complete(info) => {
            git_repo.commit_and_push();
            agent_client.complete_task(info).await.unwrap();
        }
        interaction_loop::TaskOutcome::Failure(info) => {
            agent_client.fail_task(info).await.unwrap();
        }
    }
}

fn workspace_folder_name(repo_url: &Url) -> String {
    let path = repo_url.path();
    let parts: Vec<&str> = path.split('/').collect();
    let repo_name = parts.last().unwrap_or(&"project");
    repo_name.replace(".git", "")
}
