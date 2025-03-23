use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use bollard::container::LogOutput;
use bollard::exec::{StartExecOptions, StartExecResults};
use bollard::{image::CreateImageOptions, Docker};
use futures_util::stream::TryStreamExt;
use futures_util::StreamExt;
use rand::{distributions::Alphanumeric, Rng};

pub struct Container {
    docker: Docker,
    id: String,
    workspace_dir_container: String,
}

impl Container {
    pub async fn start<P1: AsRef<Path>>(workspace_dir_host: P1, workspace_dir_name: &str) -> Self {
        let workspace_dir = workspace_dir_host.as_ref();
        let workspace_dir_container = format!("/workspaces/{}", workspace_dir_name);

        // Check for a devcontainer configuration
        let devcontainer =
            devcontainer::load(workspace_dir).expect("Failed to load devcontainer.json");
        let docker_image = devcontainer.image.expect("No image specified in devcontainer.json");

        let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker");
        let mut create_image = docker.create_image(
            Some(CreateImageOptions { from_image: docker_image.clone(), ..Default::default() }),
            None,
            None,
        );
        while let Some(_status) = create_image.try_next().await.unwrap() {}

        let config = bollard::container::Config {
            image: Some(docker_image),
            host_config: Some(bollard::models::HostConfig {
                binds: Some(vec![format!(
                    "{}:{}",
                    workspace_dir.canonicalize().unwrap().to_str().unwrap(),
                    workspace_dir_container
                )]),
                ..Default::default()
            }),
            // Ensure the container stays running
            tty: Some(true),
            cmd: Some(vec!["tail".to_owned(), "-f".to_owned(), "/dev/null".to_owned()]),
            ..Default::default()
        };

        let response = docker
            .create_container(
                Some(bollard::container::CreateContainerOptions {
                    name: "minion-devcontainer",
                    platform: None,
                }),
                config,
            )
            .await
            .expect("Failed to create container");

        docker
            .start_container(
                &response.id,
                None::<bollard::container::StartContainerOptions<String>>,
            )
            .await
            .expect("Failed to start container");

        Self { docker, id: response.id, workspace_dir_container }
    }

    pub fn workspace_dir_container(&self) -> &str {
        &self.workspace_dir_container
    }

    pub async fn run_script(&self, code: &str) -> Output {
        // Generate a unique filename for the script
        let random_str: String =
            rand::thread_rng().sample_iter(&Alphanumeric).take(16).map(char::from).collect();

        let script_filename = format!("minion-script-{}.sh", random_str);
        let script_path_container = format!("/tmp/{}", script_filename);

        // Create a tar archive containing the script file
        let mut tar_buffer = Vec::new();
        {
            let mut tar_builder = tar::Builder::new(&mut tar_buffer);
            let mut header = tar::Header::new_gnu();
            header.set_size(code.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            // Convert absolute path to relative path for the tar archive
            let script_path_in_tar =
                script_path_container.strip_prefix('/').unwrap_or(&script_path_container);
            tar_builder
                .append_data(&mut header, script_path_in_tar, code.as_bytes())
                .expect("Failed to append data to tar archive");
            tar_builder.finish().expect("Failed to finish tar archive");
        }

        // Upload the script to the container
        let options =
            bollard::container::UploadToContainerOptions { path: "/", ..Default::default() };
        self.docker
            .upload_to_container(&self.id, Some(options), tar_buffer.into())
            .await
            .expect("Failed to upload script to container");

        // Execute the script in the container
        let config = bollard::exec::CreateExecOptions {
            cmd: Some(vec!["/bin/bash", &script_path_container]),
            working_dir: Some(self.workspace_dir_container()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec_instance = self
            .docker
            .create_exec(&self.id, config)
            .await
            .expect("Failed to create exec instance");
        let exec_id = exec_instance.id;

        let start_options = StartExecOptions { detach: false, tty: false, output_capacity: None };

        let StartExecResults::Attached { mut output, .. } = self
            .docker
            .start_exec(&exec_id, Some(start_options))
            .await
            .expect("Failed to start exec")
        else {
            panic!("Failed to start exec in attached mode")
        };

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        while let Some(msg) = output.next().await {
            match msg.expect("Failed to read exec output") {
                LogOutput::StdOut { message } => stdout.extend_from_slice(&message),
                LogOutput::StdErr { message } => stderr.extend_from_slice(&message),
                _ => {}
            }
        }

        let exec_inspect =
            self.docker.inspect_exec(&exec_id).await.expect("Failed to inspect exec");

        let exit_code = exec_inspect.exit_code.unwrap_or(0);

        Output {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
        }
    }

    pub async fn read_file<P: AsRef<Path>>(&self, file_path: P) -> Result<String, ReadFileError> {
        let file_path = self.resolve_path(file_path);

        let options =
            bollard::container::DownloadFromContainerOptions { path: file_path.to_str().unwrap() };

        let mut stream = self.docker.download_from_container(&self.id, Some(options));

        let mut bytes = Vec::new();
        loop {
            match stream.try_next().await {
                Ok(Some(chunk)) => {
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    if let bollard::errors::Error::DockerResponseServerError {
                        status_code, ..
                    } = &e
                    {
                        if *status_code == 404 {
                            // File not found
                            return Err(ReadFileError::NotFound);
                        }
                    }
                    // Other errors
                    return Err(ReadFileError::Other(e.to_string()));
                }
            }
        }

        let mut archive = tar::Archive::new(io::Cursor::new(bytes));
        let mut content = String::new();

        if let Some(entry) =
            archive.entries().map_err(|e| ReadFileError::Other(e.to_string()))?.next()
        {
            let mut file = entry.map_err(|e| ReadFileError::Other(e.to_string()))?;
            file.read_to_string(&mut content).map_err(|e| ReadFileError::Other(e.to_string()))?;
        } else {
            return Err(ReadFileError::NotFound);
        }

        Ok(content)
    }

    pub async fn write_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        content: &str,
    ) -> Result<(), String> {
        let file_path = self.resolve_path(file_path);

        // Create a tar archive containing the file and necessary directories
        let mut tar_buffer = Vec::new();
        {
            let mut tar_builder = tar::Builder::new(&mut tar_buffer);

            // Collect all parent directories of the file path
            let mut dirs = Vec::new();
            let mut current = file_path.parent();
            while let Some(parent) = current {
                dirs.push(parent.to_path_buf());
                current = parent.parent();
            }
            // Reverse to ensure directories are created from root to leaf
            dirs.reverse();

            // Add directory entries to the tar archive
            for dir in dirs {
                let dir_path = dir.strip_prefix("/").unwrap_or(&dir);
                if !dir_path.as_os_str().is_empty() {
                    let mut header = tar::Header::new_gnu();
                    header.set_path(dir_path).map_err(|e| e.to_string())?;
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_mode(0o755);
                    header.set_size(0);
                    header.set_cksum();
                    tar_builder.append(&header, &[] as &[u8]).map_err(|e| e.to_string())?;
                }
            }

            // Add the file entry to the tar archive
            let file_path_in_tar = file_path.strip_prefix("/").unwrap_or(&file_path);
            let mut header = tar::Header::new_gnu();
            header.set_path(file_path_in_tar).map_err(|e| e.to_string())?;
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append_data(&mut header, file_path_in_tar, content.as_bytes())
                .map_err(|e| e.to_string())?;
            tar_builder.finish().map_err(|e| e.to_string())?;
        }

        // Upload the tar archive to the container
        let options = bollard::container::UploadToContainerOptions {
            path: "/", // Extract at the root of the container's filesystem
            ..Default::default()
        };

        self.docker
            .upload_to_container(&self.id, Some(options), tar_buffer.into())
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn resolve_path<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_owned()
        } else {
            Path::new(&self.workspace_dir_container).join(path)
        }
    }
}

pub struct Output {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

pub enum ReadFileError {
    NotFound,
    Other(String),
}
