use agent_api::types::task::{Task, TaskComplete, TaskFailure, TaskFailureReason, TaskStatus};

use crate::actions::files::{read_file, write_file};
use crate::actions::markdown::strip_wrapping_markdown_code_fences;
use crate::container::{Container, Output, ReadFileError};
use crate::llm::{self, Prompt, PromptItem};

use super::history::History;
use super::resources::Resources;

const SMART_MODEL: &str = "o1-mini";
const BASIC_MODEL: &str = "gpt-4o-mini";

const INTRO_1: &str = r#"You are an autonomous agent that solves coding tasks.
You keep your explanations as concise as possible.
You are connected to a Linux-based development environment. You are in the project directory.
Your current task is as follows:"#;

const INTRO_2: &str = r#"In order to complete the task, the system will guide you through a series of actions.
In each action, you will be able to interact with the environment using the following actions:

* `bash`: Execute bash code
* `read-file`: Read the contents of a file
* `edit-file`: Read, and optionally replace the contents of a file
* `end-task`: End your task because it is completed, or because there is an insurmountable issue preventing you from completing it.

You will be instructed when to choose an action.
You can use the `bash` action to install and execute arbitrary command line tools that are helpful for your task.
You can use `ls` or `tree` to explore the file system, or `curl` to download files.
You do not need to use `sudo` as you are already running as a privileged user.
"#;

pub enum TaskOutcome {
    Complete(TaskComplete),
    Failure(TaskFailure),
}

pub async fn run(llm_client: &llm::LLMClient, container: &Container, task: &Task) -> TaskOutcome {
    let mut resources = Resources::default();

    assert_eq!(task.status, TaskStatus::Running);

    let prefix = vec![
        PromptItem::System { text: INTRO_1.to_owned() },
        PromptItem::User { content: task.description.to_owned().into() },
        PromptItem::System { text: INTRO_2.to_owned() },
    ];

    let mut history = History::new(prefix);

    loop {
        let action_result =
            single_action(llm_client, container, &mut history, &mut resources).await;
        match action_result {
            ActionResult::EndTask(outcome) => break outcome,
            ActionResult::Continue => continue,
        }
    }
}

async fn summarize_action(
    prompt: &Prompt,
    llm_client: &llm::LLMClient,
    action_number: usize,
) -> String {
    let mut prompt = prompt.clone();
    let summarize_message = format!("Summarize what you have done in action {}.", action_number);
    prompt.items.push(PromptItem::System { text: summarize_message });
    llm_client.prompt(BASIC_MODEL, &prompt).await.unwrap()
}

const DISCUSS_FIRST: &str = r#"Plan the first step of your approach without writing any code, yet.
Let's think step by step."#;

const DISCUSS_BASH: &str = r#"Discuss what the output means.
Then, plan what you want to do next without writing any code, yet.
Let's think step by step."#;

const DISCUSS_READ_FILE: &str = r#"Discuss the file content.
Then, plan what you want to do next without writing any code, yet.
Let's think step by step."#;

const DISCUSS_EDIT_FILE: &str = r#"Discuss your edits.
Then, plan what you want to do next without writing any code, yet.
Let's think step by step."#;

pub enum ActionResult {
    EndTask(TaskOutcome),
    Continue,
}

async fn single_action(
    llm_client: &llm::LLMClient,
    container: &Container,
    history: &mut History,
    resources: &mut Resources,
) -> ActionResult {
    let mut p = history.compressed_prompt();
    let action_number = history.actions.len();
    let start_idx = p.items.len();
    p.items.push(PromptItem::System { text: format!("BEGIN ACTION {}", action_number) });

    if action_number == 0 {
        p.items.push(PromptItem::System { text: DISCUSS_FIRST.to_owned() });
        let completion = llm_client.prompt(SMART_MODEL, &p).await.unwrap();
        p.items.push(PromptItem::Assistant { text: completion });
    }

    let action = select_action(llm_client, &mut p).await;

    match action {
        Action::Bash => {
            action_bash(llm_client, container, &mut p).await;
            p.items.push(PromptItem::System { text: DISCUSS_BASH.to_owned() });
        }
        Action::ReadFile => {
            action_read_file(llm_client, container, &mut p, resources).await;
            p.items.push(PromptItem::System { text: DISCUSS_READ_FILE.to_owned() });
        }
        Action::EditFile => {
            action_edit_file(llm_client, container, &mut p, resources).await;
            p.items.push(PromptItem::System { text: DISCUSS_EDIT_FILE.to_owned() });
        }
        Action::EndTask => {
            return action_end_task(llm_client, &mut p).await;
        }
    }

    let completion = llm_client.prompt(SMART_MODEL, &p).await.unwrap();
    p.items.push(PromptItem::Assistant { text: completion });

    p.items.push(PromptItem::System { text: format!("END ACTION {}", action_number) });

    let summary = summarize_action(&p, llm_client, action_number).await;
    history.append(p.items[start_idx..].to_vec(), summary);

    ActionResult::Continue
}

enum Action {
    Bash,
    ReadFile,
    EditFile,
    EndTask,
}

const DISCUSS_ACTION: &str = r#"To realize the first step of your plan, you must now choose one of the following actions:

* `bash`: Execute bash code
* `read-file`: Read the contents of a file
* `edit-file`: Read, and optionally replace the contents of a file
* `end-task`: End your task because it is completed, or because there is an insurmountable issue preventing you from completing it.

To write code, you must use the `edit-file` action.
Discuss which action you choose. Let's think step by step.
"#;

const SELECT_ACTION: &str = r#"Give the name of the action you chose above.
No prose, your message must consist solely of the action name.
For instance, if you chose the bash action, you would write:

bash
"#;

async fn select_action(llm_client: &llm::LLMClient, prompt: &mut Prompt) -> Action {
    prompt.items.push(PromptItem::System { text: DISCUSS_ACTION.to_owned() });
    let completion = llm_client.prompt(BASIC_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: completion });
    prompt.items.push(PromptItem::System { text: SELECT_ACTION.to_owned() });
    let completion = llm_client.prompt(BASIC_MODEL, prompt).await.unwrap();
    match completion.as_str() {
        "bash" => Action::Bash,
        "read-file" => Action::ReadFile,
        "edit-file" => Action::EditFile,
        "end-task" => Action::EndTask,
        _ => panic!("Unexpected action: {}", completion),
    }
}

const ACTION_BASH: &str = r#"Provide the bash script you want to run.
No prose. Your message should only consist of bash code:
"#;

async fn action_bash(llm_client: &llm::LLMClient, container: &Container, prompt: &mut Prompt) {
    prompt.items.push(PromptItem::System { text: ACTION_BASH.to_owned() });
    let code = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: code.clone() });

    let code = strip_wrapping_markdown_code_fences(&code);

    let Output { stdout, stderr, exit_code } = container.run_script(&code).await;

    let msg = format!(
        "Stdout: \n```\n{}\n```\nStderr: \n```\n{}\n```\nExit status: {}\n",
        stdout, stderr, exit_code
    );
    prompt.items.push(PromptItem::System { text: msg });
}

const ACTION_EDIT_FILEPATH: &str = r#"Provide the path of the file you want to edit.
No prose. Your message should only consist of the filepath.
For instance, to read `foo/bar/example.txt`, write:

foo/bar/example.txt
"#;

const ACTION_EDIT_DISCUSS: &str =
    r#"Discuss whether you want to edit the file and if so, which changes you want to make."#;

const ACTION_EDIT_REPLACE: &str = r#"Provide the file content with your edits applied.
If you do not want to edit the file, restate the current file contents.
No prose. Your message must list the whole updated file, because the file will be overwritten with your new content:
"#;

const ACTION_EDIT_CREATE: &str = r#"Provide the new file contents.
No prose. Do not wrap the file contents in markdown code fences that are not part of the file contents themselves.
Your message must only consist of the new file contents:
"#;

const ACTION_EDITED: &str = r#"The edited file has been saved."#;

async fn action_edit_file(
    llm_client: &llm::LLMClient,
    container: &Container,
    prompt: &mut Prompt,
    resources: &mut Resources,
) {
    prompt.items.push(PromptItem::System { text: ACTION_EDIT_FILEPATH.to_owned() });
    let filepath = llm_client.prompt(BASIC_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: filepath.clone() });

    let content = match read_file(container, &filepath).await {
        Ok(content) => content,
        Err(ReadFileError::NotFound) => {
            prompt.items.push(PromptItem::System {
                text: "The file does not exist. It will be created.".to_owned(),
            });
            prompt.items.push(PromptItem::System { text: ACTION_EDIT_CREATE.to_owned() });
            let contents = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
            prompt.items.push(PromptItem::Assistant { text: contents.clone() });
            resources.add_file(&filepath);
            write_file(container, &filepath, &contents).await;
            prompt.items.push(PromptItem::System { text: ACTION_EDITED.to_owned() });
            return;
        }
        Err(ReadFileError::Other(err)) => {
            prompt.items.push(PromptItem::System {
                text: format!("An error occured while reading the file: {}", err),
            });
            return;
        }
    };

    resources.add_file(&filepath);

    prompt.items.push(PromptItem::System { text: format!("The content of `{}` is:", filepath) });
    prompt.items.push(PromptItem::System { text: content });
    prompt.items.push(PromptItem::System { text: ACTION_EDIT_DISCUSS.to_owned() });
    let completion = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: completion });
    prompt.items.push(PromptItem::System { text: ACTION_EDIT_REPLACE.to_owned() });
    let contents = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: contents.clone() });
    write_file(container, &filepath, &contents).await;
    prompt.items.push(PromptItem::System { text: ACTION_EDITED.to_owned() });
}

const ACTION_READ_FILEPATH: &str = r#"Provide the path of the file you want to read.
No prose. Your message must only consist of the filepath.
For instance, to read `foo/bar/example.txt`, write:

foo/bar/example.txt
"#;

async fn action_read_file(
    llm_client: &llm::LLMClient,
    container: &Container,
    prompt: &mut Prompt,
    resources: &mut Resources,
) {
    prompt.items.push(PromptItem::System { text: ACTION_READ_FILEPATH.to_owned() });
    let filepath = llm_client.prompt(BASIC_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: filepath.clone() });

    let content = match read_file(container, &filepath).await {
        Ok(content) => content,
        Err(ReadFileError::NotFound) => {
            prompt.items.push(PromptItem::System { text: "The file does not exist.".to_owned() });
            return;
        }
        Err(ReadFileError::Other(err)) => {
            prompt.items.push(PromptItem::System {
                text: format!("An error occured while reading the file: {}", err),
            });
            return;
        }
    };
    resources.add_file(&filepath);

    prompt.items.push(PromptItem::System { text: format!("The content of `{}` is:", filepath) });
    prompt.items.push(PromptItem::System { text: content });
}

const ACTION_END_TASK_DISCUSS: &str = r#"You have decided to end the task.
Discuss whether you have completed the task or if there is an issue preventing you from completing it.
Afterwards, you will be able to select one of the following exit statuses:

* `complete`: The task is completed.
* `failure`: The task is failed.

"#;

const ACTION_END_TASK_SELECT: &str = r#"Give the name of the exit status you chose above.
No prose, your message must consist solely of the action name.
For instance, if you chose to mark the task as complete, you would write:

complete
"#;

const ACTION_COMPLETE_TASK_DESCRIPTION: &str = r#"Give a final summary of the task which will be displayed to the user.

The summary should discuss the task, the steps you took to complete it, and the final result. Be concise.
"#;

const ACTION_FAIL_TASK_DESCRIPTION: &str = r#"Give a final summary on why the task failed. This summary will be displayed to the user.

The summary should discuss the task, the steps you took, and the reason for the failure. Finally, you can suggest possible solutions. Be concise.
"#;

const ACTION_FAIL_TASK_REASON_DISCUSS: &str = r#"Please categorize the reason for task failure.
You will be able to select one of the following categories:

* `technical-issues`: You failed to complete the task due to technical problems unrelated to the task itself
* `task-issues`: You failed to complete the task due to a problem with the task itself, e.g. because the task was unclear or impossible to complete
* `problem-solving`: There were no fundamental technical issues and the task was valid, but you still failed to complete the task because you did not succeed at task-specific problem-solving.

Discuss which category you choose. Let's think step by step.
"#;

const ACTION_FAIL_TASK_REASON_SELECT: &str = r#"Give the name of the reason category you chose above.
No prose, your message must consist solely of the reason category name.

For instance, if you chose the technical-issues category, you would write:

technical-issues
"#;

async fn action_end_task(llm_client: &llm::LLMClient, prompt: &mut Prompt) -> ActionResult {
    prompt.items.push(PromptItem::System { text: ACTION_END_TASK_DISCUSS.to_owned() });
    let completion = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: completion });

    prompt.items.push(PromptItem::System { text: ACTION_END_TASK_SELECT.to_owned() });
    let completion = llm_client.prompt(BASIC_MODEL, prompt).await.unwrap();
    prompt.items.push(PromptItem::Assistant { text: completion.clone() });

    let outcome = match completion.as_str() {
        "complete" => {
            prompt
                .items
                .push(PromptItem::System { text: ACTION_COMPLETE_TASK_DESCRIPTION.to_owned() });
            let description = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
            TaskOutcome::Complete(TaskComplete { description })
        }
        "failure" => {
            prompt.items.push(PromptItem::System { text: ACTION_FAIL_TASK_DESCRIPTION.to_owned() });
            let description = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
            prompt.items.push(PromptItem::Assistant { text: description.clone() });

            prompt
                .items
                .push(PromptItem::System { text: ACTION_FAIL_TASK_REASON_DISCUSS.to_owned() });
            let completion = llm_client.prompt(SMART_MODEL, prompt).await.unwrap();
            prompt.items.push(PromptItem::Assistant { text: completion.clone() });

            prompt
                .items
                .push(PromptItem::System { text: ACTION_FAIL_TASK_REASON_SELECT.to_owned() });
            let reason_str = llm_client.prompt(BASIC_MODEL, prompt).await.unwrap();

            let reason = match reason_str.as_str() {
                "technical-issues" => Some(TaskFailureReason::TechnicalIssues),
                "task-issues" => Some(TaskFailureReason::TaskIssues),
                "problem-solving" => Some(TaskFailureReason::ProblemSolving),
                _ => None,
            };

            TaskOutcome::Failure(TaskFailure { reason, description })
        }
        _ => panic!("Unknown task ending choice: {}", completion),
    };

    ActionResult::EndTask(outcome)
}
