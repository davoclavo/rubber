use log::{debug, error, info, warn};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::error::Error;
use std::io::{self, BufRead, Write};

#[derive(Deserialize, Debug)]
struct PullRequest {
    number: u32,
    title: String,
    body: Option<String>,
    user: User,
    created_at: String,
    html_url: String,
    comments_url: String,
    url: String,
}

#[derive(Deserialize, Debug)]
struct User {
    login: String,
}

#[derive(Deserialize, Debug)]
struct Comment {
    id: u64,
    user: User,
    created_at: String,
    body: String,
}

#[derive(Serialize, Debug)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Serialize, Debug)]
struct ClaudeRequest {
    model: String,
    messages: Vec<ClaudeMessage>,
    max_tokens: u32,
}

#[derive(Default)]
struct OutputBuffer {
    content: String,
}

impl OutputBuffer {
    fn new() -> Self {
        Self::default()
    }

    fn add_line(&mut self, line: impl AsRef<str>) {
        self.content.push_str(line.as_ref());
        self.content.push('\n');
    }

    fn add_separator(&mut self, ch: char, count: usize) {
        self.add_line(&ch.to_string().repeat(count));
    }
}

fn get_comments_count(comments_url: &str) -> Result<usize, Box<dyn Error>> {
    let response = ureq::get(comments_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    let comments: Vec<Value> = serde_json::from_str(&response)?;
    Ok(comments.len())
}

fn get_pr_comments(comments_url: &str) -> Result<Vec<Comment>, Box<dyn Error>> {
    let response = ureq::get(comments_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    let comments: Vec<Comment> = serde_json::from_str(&response)?;
    Ok(comments)
}

#[derive(Deserialize, Debug)]
struct PullRequestDetail {
    title: String,
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    files: Vec<FileChange>,
}

#[derive(Deserialize, Debug)]
struct FileChange {
    filename: String,
    status: String,
    additions: u32,
    deletions: u32,
    changes: u32,
    patch: Option<String>,
}

async fn get_code_review(patch: &str) -> Result<String, Box<dyn Error>> {
    let api_key =
        env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY environment variable not set");

    let prompt = format!(
        "Please review this code patch and provide specific, actionable feedback about potential issues, \
        improvements, and best practices. Consider performance, security, maintainability, and Rust idioms.\n\n\
        ```\n{}\n```",
        patch
    );

    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(&api_key)?);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let messages = vec![ClaudeMessage {
        role: "user".to_string(),
        content: prompt,
    }];

    let request = ClaudeRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages,
        max_tokens: 1000,
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    debug!("Request: {:?}", request);
    debug!("Response: {:?}", response);

    let review = response["content"][0]["text"]
        .as_str()
        .ok_or("Failed to get response text")?
        .to_string();

    Ok(review)
}

async fn analyze_patch(patch: &str, output: &mut OutputBuffer) -> Result<(), Box<dyn Error>> {
    // Count additions and deletions
    let additions = patch.lines().filter(|l| l.starts_with('+')).count();
    let deletions = patch.lines().filter(|l| l.starts_with('-')).count();

    output.add_line(&format!(
        "Changed {} lines ({} additions, {} deletions)",
        additions + deletions,
        additions,
        deletions
    ));

    // Look for common patterns that might need attention
    if patch.contains("TODO") || patch.contains("FIXME") {
        output.add_line("\nQuestions to consider:");
        output.add_line(
            "- There are TODOs/FIXMEs in the code - should these be addressed before merging?",
        );
    }

    if patch.contains("println!") || patch.contains("dbg!") {
        output.add_line("- Debug print statements found - are these intended for production?");
    }

    // Look for potential improvements
    let mut has_comments = false;
    if patch.contains("unwrap()") {
        if !has_comments {
            output.add_line("\nPotential feedback:");
            has_comments = true;
        }
        output.add_line("- Consider handling errors explicitly instead of using unwrap()");
    }

    if patch.contains("panic!") {
        if !has_comments {
            output.add_line("\nPotential feedback:");
        }
        output.add_line(
            "- Consider if panic! is appropriate here or if errors should be handled gracefully",
        );
    }

    // Add Claude's review if available
    match get_code_review(patch).await {
        Ok(review) => {
            output.add_line("\nClaude's Review:");
            output.add_separator('-', 80);
            output.add_line(&review);
            output.add_separator('-', 80);
        }
        Err(e) => {
            error!("Error getting code review: {}", e);
        }
    }

    Ok(())
}

fn get_pr_details(
    pr_number: u32,
    owner: &str,
    repo: &str,
) -> Result<PullRequestDetail, Box<dyn Error>> {
    // The correct endpoint for PR details
    let pr_url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}",
        owner, repo, pr_number
    );

    // Fetch PR details
    let pr_response = ureq::get(&pr_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    let mut details: PullRequestDetail = serde_json::from_str(&pr_response)?;

    // Fetch files separately using the files endpoint
    let files_url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}/files",
        owner, repo, pr_number
    );

    let files_response = ureq::get(&files_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    details.files = serde_json::from_str(&files_response)?;

    Ok(details)
}

async fn display_pr_details(
    pr: &PullRequest,
    owner: &str,
    repo: &str,
    output: &mut OutputBuffer,
) -> Result<(), Box<dyn Error>> {
    output.add_separator('=', 80);
    output.add_line(&format!("PR #{}: {}", pr.number, pr.title));
    output.add_separator('=', 80);

    output.add_line("\nDescription:");
    output.add_separator('-', 80);
    if let Some(body) = &pr.body {
        if !body.trim().is_empty() {
            output.add_line(body);
        } else {
            output.add_line("No description provided.");
        }
    } else {
        output.add_line("No description provided.");
    }
    output.add_separator('-', 80);

    match get_pr_details(pr.number, owner, repo) {
        Ok(details) => {
            output.add_line("\nModified Files:");
            output.add_separator('-', 80);

            if details.files.is_empty() {
                output.add_line("No files modified in this PR.");
            } else {
                output.add_line(&format!(
                    "{:<50} {:<10} {:<10} {:<10}",
                    "Filename", "Status", "Additions", "Deletions"
                ));
                for file in details.files {
                    output.add_line(&format!(
                        "{:<50} {:<10} {:<10} {:<10}",
                        file.filename, file.status, file.additions, file.deletions
                    ));

                    if let Some(patch) = file.patch {
                        output.add_line(&format!("\nDiff for {}:", file.filename));
                        output.add_separator('-', 80);
                        output.add_line(format!("{}", patch));
                        output.add_line("\nAnalysis:");
                        analyze_patch(&patch, output).await?;
                    }
                }
            }
            output.add_separator('-', 80);
        }
        Err(e) => {
            error!("\nError fetching PR details: {}", e);
            output.add_line("\nError fetching PR details.");
            output.add_line("Unable to display modified files.");
        }
    }

    Ok(())
}

fn display_pr_comments(
    pr_number: u32,
    owner: &str,
    repo: &str,
    output: &mut OutputBuffer,
) -> Result<(), Box<dyn Error>> {
    let comments_url = format!(
        "https://api.github.com/repos/{}/{}/issues/{}/comments",
        owner, repo, pr_number
    );

    let comments = get_pr_comments(&comments_url)?;

    output.add_line(&format!("\nComments for PR #{}:", pr_number));

    if comments.is_empty() {
        output.add_line("No comments found for this PR.");
    } else {
        output.add_separator('-', 80);
        for comment in comments {
            output.add_line(&format!(
                "Author: {} (at {})",
                comment.user.login, comment.created_at
            ));
            output.add_separator('-', 80);
            output.add_line(&comment.body);
            output.add_separator('-', 80);
            output.add_line("");
        }
    }

    Ok(())
}

fn find_pr_by_number(prs: &[PullRequest], number: u32) -> Option<&PullRequest> {
    prs.iter().find(|pr| pr.number == number)
}

async fn run() -> Result<String, Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    let mut output = OutputBuffer::new();
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        error!("Usage: {} <owner> <repo> [pr_number]", args[0]);
        std::process::exit(1);
    }

    let owner = &args[1];
    let repo = &args[2];

    // If PR number is provided, show its details directly
    if let Some(pr_number) = args.get(3) {
        match pr_number.parse::<u32>() {
            Ok(number) => {
                let pr_url = format!(
                    "https://api.github.com/repos/{}/{}/pulls/{}",
                    owner, repo, number
                );

                let pr = ureq::get(&pr_url)
                    .set("User-Agent", "rubber")
                    .call()?
                    .into_json::<PullRequest>()?;

                display_pr_details(&pr, owner, repo, &mut output).await?;
                display_pr_comments(number, owner, repo, &mut output)?;
                return Ok(output.content);
            }
            Err(_) => {
                error!("Invalid PR number: {}", pr_number);
                return Ok(format!("Invalid PR number: {}", pr_number));
            }
        }
    }

    output.add_line(&format!(
        "Fetching the 10 most recent PRs for {}/{}",
        owner, repo
    ));

    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls?state=all&sort=created&direction=desc&per_page=10",
        owner, repo
    );

    let response = ureq::get(&url)
        .set("User-Agent", "rubber")
        .call()?
        .into_json::<Vec<PullRequest>>()?;

    if response.is_empty() {
        output.add_line("No pull requests found.");
        return Ok(output.content);
    } else {
        output.add_line(&format!(
            "{:<6} {:<50} {:<20} {:<15} {:<15}",
            "PR#", "Title", "Author", "Created At", "Comments"
        ));
        output.add_line(&"-".repeat(106));

        for pr in &response {
            // Truncate title if too long
            let title = if pr.title.len() > 47 {
                format!("{}...", &pr.title[..44])
            } else {
                pr.title.clone()
            };

            // Fetch comment count for this PR
            let comments_count = match get_comments_count(&pr.comments_url) {
                Ok(count) => count.to_string(),
                Err(_) => "Error".to_string(),
            };

            output.add_line(&format!(
                "{:<6} {:<50} {:<20} {:<15} {:<15}",
                pr.number, title, pr.user.login, pr.created_at, comments_count
            ));

            // Print the PR URL on a separate line
            output.add_line(&format!("       URL: {}", pr.html_url));
        }

        // Print the accumulated output before asking for input
        print!("{}", output.content);
        io::stdout().flush()?;

        // Clear the output buffer since we've printed it
        output.content.clear();

        output.add_line("\nEnter PR number to view details (or 'q' to quit): ");
        print!("{}", output.content);
        io::stdout().flush()?;

        let stdin = io::stdin();
        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;

        // Clear the output buffer again for the next phase
        output.content.clear();

        let input = input.trim();
        if input.to_lowercase() != "q" {
            match input.parse::<u32>() {
                Ok(pr_number) => {
                    if let Some(pr) = find_pr_by_number(&response, pr_number) {
                        display_pr_details(pr, owner, repo, &mut output).await?;
                        display_pr_comments(pr_number, owner, repo, &mut output)?;
                        return Ok(output.content);
                    } else {
                        warn!("PR #{} not found in the current list.", pr_number);
                        return Ok(format!("PR #{} not found in the current list.", pr_number));
                    }
                }
                Err(_) => warn!("Invalid PR number."),
            }
        }
    }

    Ok(output.content)
}

#[tokio::main]
async fn main() {
    // Run the main logic and store the result
    match run().await {
        Ok(output) => {
            // Print the accumulated output
            print!("{}", output);
            // Flush stdout to ensure everything is printed
            io::stdout().flush().unwrap();
        }
        Err(e) => {
            log::error!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
