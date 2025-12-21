use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionNamedToolChoice,
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionToolArgs,
        ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionCall, FunctionName, FunctionObjectArgs,
    },
};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use log::{error, info};
use question::{Answer, Question};
use rand::seq::SliceRandom;
use schemars::{
    gen::{SchemaGenerator, SchemaSettings},
    JsonSchema,
};
use serde_json::json;
use spinners::{Spinner, Spinners};
use std::{
    io::Write,
    process::{Command, Stdio},
    str,
};

#[derive(Parser)]
#[command(version)]
#[command(name = "Auto Commit")]
#[command(author = "Miguel Piedrafita <soy@miguelpiedrafita.com>")]
#[command(about = "Automagically generate commit messages.", long_about = None)]
struct Cli {
    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,

    #[arg(
        long = "dry-run",
        help = "Output the generated message, but don't create a commit."
    )]
    dry_run: bool,

    #[arg(
        short,
        long,
        help = "Edit the generated commit message before committing."
    )]
    review: bool,

    #[arg(short, long, help = "Don't ask for confirmation before committing.")]
    force: bool,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
struct Commit {
    /// The title of the commit.
    title: String,

    /// An exhaustive description of the changes.
    description: String,
}

impl ToString for Commit {
    fn to_string(&self) -> String {
        format!("{}\n\n{}", self.title, self.description)
    }
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    let cli = Cli::parse();
    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .format(|buf, record| writeln!(buf, "{}", record.args()))
        .init();

    let api_token = std::env::var("OPENAI_KEY").unwrap_or_else(|_| {
        error!("Please set the OPENAI_KEY environment variable.");
        std::process::exit(1);
    });

    let git_staged_cmd = Command::new("git")
        .arg("diff")
        .arg("--staged")
        .output()
        .expect("Couldn't find diff.")
        .stdout;

    let git_staged_cmd = str::from_utf8(&git_staged_cmd).unwrap();

    if git_staged_cmd.is_empty() {
        error!("There are no staged files to commit.\nTry running `git add` to stage some files.");
    }

    let is_repo = Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
        .expect("Failed to check if this is a git repository.")
        .stdout;

    if str::from_utf8(&is_repo).unwrap().trim() != "true" {
        error!("It looks like you are not in a git repository.\nPlease run this command from the root of a git repository, or initialize one using `git init`.");
        std::process::exit(1);
    }

    let branch_output = Command::new("git")
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("Failed to get branch")
        .stdout;

    let branch = str::from_utf8(&branch_output).unwrap().trim();

    let client = async_openai::Client::with_config(OpenAIConfig::new().with_api_key(api_token));

    let output = Command::new("git")
        .arg("diff")
        .arg("--staged")
        .arg("HEAD")
        .output()
        .expect("Couldn't find diff.")
        .stdout;
    let output = str::from_utf8(&output).unwrap();

    if !cli.dry_run {
        info!("Loading Data...");
    }

    let sp: Option<Spinner> = if !cli.dry_run && cli.verbose.is_silent() {
        let vs = [
            Spinners::Earth,
            Spinners::Aesthetic,
            Spinners::Hearts,
            Spinners::BoxBounce,
            Spinners::BoxBounce2,
            Spinners::BouncingBar,
            Spinners::Christmas,
            Spinners::Clock,
            Spinners::FingerDance,
            Spinners::FistBump,
            Spinners::Flip,
            Spinners::Layer,
            Spinners::Line,
            Spinners::Material,
            Spinners::Mindblown,
            Spinners::Monkey,
            Spinners::Noise,
            Spinners::Point,
            Spinners::Pong,
            Spinners::Runner,
            Spinners::SoccerHeader,
            Spinners::Speaker,
            Spinners::SquareCorners,
            Spinners::Triangle,
        ];

        let spinner = vs.choose(&mut rand::thread_rng()).unwrap().clone();

        Some(Spinner::new(spinner, "Analyzing Codebase...".into()))
    } else {
        None
    };

    let mut generator = SchemaGenerator::new(SchemaSettings::openapi3().with(|settings| {
        settings.inline_subschemas = true;
    }));

    let commit_schema = generator.subschema_for::<Commit>().into_object();

    let completion = client
        .chat()
        .create(
            CreateChatCompletionRequestArgs::default()
                .messages(vec![
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(format!(
                            "You are an experienced programmer who writes great commit messages. \
                             Prefix the title with the branch followed by a colon. {}: Detailed description of the changes.",
                            branch
                        ))
                        .build()
                        .unwrap()
                        .into(),
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .tool_calls(vec![ChatCompletionMessageToolCall {
                            id: "get_diff".to_string(),
                            r#type: ChatCompletionToolType::Function,
                            function: FunctionCall {
                                arguments: "{}".to_string(),
                                name: "get_diff".to_string(),
                            },
                        }])
                        .build()
                        .unwrap()
                        .into(),
                    ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id("get_diff".to_string())
                        .content(output.to_string())
                        .build()
                        .unwrap()
                        .into(),
                ])
                .tools(vec![
                    ChatCompletionToolArgs::default()
                        .r#type(ChatCompletionToolType::Function)
                        .function(
                            FunctionObjectArgs::default()
                                .name("get_diff".to_string())
                                .description(
                                    "Returns the output of `git diff --staged HEAD` as a string."
                                        .to_string(),
                                )
                                .parameters(json!({
                                    "type": "object",
                                    "properties": {}
                                }))
                                .build()
                                .unwrap(),
                        )
                        .build()
                        .unwrap(),
                    ChatCompletionToolArgs::default()
                        .r#type(ChatCompletionToolType::Function)
                        .function(
                            FunctionObjectArgs::default()
                                .name("commit".to_string())
                                .description(
                                    "Creates a commit with the given title and a description."
                                        .to_string(),
                                )
                                .parameters(serde_json::to_value(commit_schema).unwrap())
                                .build()
                                .unwrap(),
                        )
                        .build()
                        .unwrap(),
                ])
                .tool_choice(ChatCompletionToolChoiceOption::Named(
                    ChatCompletionNamedToolChoice {
                        r#type: ChatCompletionToolType::Function,
                        function: FunctionName {
                            name: "commit".to_string(),
                        },
                    },
                ))
                .model("gpt-5.2")
                .temperature(0.0)
                .max_completion_tokens(1000u32)
                .build()
                .unwrap(),
        )
        .await
        .expect("Couldn't complete prompt.");

    if sp.is_some() {
        sp.unwrap().stop_with_message("Finished Analyzing!".into());
    }

    let commit_data = &completion.choices[0].message.tool_calls;
    let commit_msg =
        serde_json::from_str::<Commit>(&commit_data.as_ref().unwrap()[0].function.arguments)
            .expect("Couldn't parse model response.")
            .to_string();

    if cli.dry_run {
        info!("{}", commit_msg);
        return Ok(());
    } else {
        info!(
            "Proposed Commit:\n------------------------------\n{}\n------------------------------",
            commit_msg
        );

        if !cli.force {
            let answer = Question::new("Do you want to continue? (Y/n)")
                .yes_no()
                .until_acceptable()
                .default(Answer::YES)
                .ask()
                .expect("Couldn't ask question.");

            if answer == Answer::NO {
                error!("Commit aborted by user.");
                std::process::exit(1);
            }
            info!("Committing Message...");
        }
    }

    let mut ps_commit = Command::new("git")
        .arg("commit")
        .args(if cli.review { vec!["-e"] } else { vec![] })
        .arg("-F")
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = ps_commit.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin
            .write_all(commit_msg.as_bytes())
            .expect("Failed to write to stdin");
    });

    let commit_output = ps_commit
        .wait_with_output()
        .expect("There was an error when creating the commit.");

    info!("{}", str::from_utf8(&commit_output.stdout).unwrap());

    Ok(())
}
