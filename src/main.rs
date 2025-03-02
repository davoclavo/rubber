use log::{error, info, trace, warn};
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

    fn add_header(&mut self, text: &str) {
        self.add_line("");
        let padding = 76_usize.saturating_sub(text.len());
        self.add_line(&format!("┏━━ {} {}", text, "━".repeat(padding)));
    }

    fn add_section(&mut self, text: &str) {
        let padding = 76_usize.saturating_sub(text.len());
        self.add_line(&format!("┣━━ {} {}", text, "━".repeat(padding)));
    }

    fn add_box_content(&mut self, content: &str) {
        self.add_line("┃");
        self.add_box_inner_content(content);
        self.add_line("┃");
    }

    fn add_box_inner_content(&mut self, content: &str) {
        for line in content.lines() {
            self.add_line(&format!("┃  {}", line));
        }
    }

    fn add_diff_header(&mut self, filename: &str) {
        self.add_line("");
        let padding = 70_usize.saturating_sub(filename.len());
        self.add_line(&format!("┏━━ Diff: {} {}", filename, "━".repeat(padding)));
    }

    fn add_diff_content(&mut self, content: &str) {
        for line in content.lines() {
            let formatted_line = match line.chars().next() {
                Some('+') => format!("┃  \x1b[32m{}\x1b[0m", line), // Green for additions
                Some('-') => format!("┃  \x1b[31m{}\x1b[0m", line), // Red for deletions
                _ => format!("┃  {}", line),
            };
            self.add_line(&formatted_line);
        }
    }

    fn add_diff_separator(&mut self) {
        self.add_line(
            "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
        );
    }
}

fn get_comments_count(
    comments_url: &str,
    github_token: Option<&str>,
) -> Result<usize, Box<dyn Error>> {
    let mut request = ureq::get(comments_url).set("User-Agent", "rubber");

    if let Some(token) = github_token {
        request = request.set("Authorization", &format!("Bearer {}", token));
    }

    let response = request.call()?.into_string()?;

    let comments: Vec<Value> = serde_json::from_str(&response)?;
    Ok(comments.len())
}

fn get_pr_comments(
    comments_url: &str,
    github_token: Option<&str>,
) -> Result<Vec<Comment>, Box<dyn Error>> {
    let mut request = ureq::get(comments_url).set("User-Agent", "rubber");

    if let Some(token) = github_token {
        request = request.set("Authorization", &format!("Bearer {}", token));
    }

    let response = request.call()?.into_string()?;

    let comments: Vec<Comment> = serde_json::from_str(&response)?;
    Ok(comments)
}

#[derive(Deserialize, Debug)]
struct PullRequestDetail {
    title: String,
    body: Option<String>,
    html_url: String,
    user: User,
    created_at: String,
    comments_url: String,
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
    info!("Generating AI review for patch...");

    let api_key =
        env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY environment variable not set");

    let prompt = format!(
        "Review this code patch and provide:\n\
        1. A brief summary of the changes (2-3 sentences)\n\
        2. Specific issues or needed improvements, focusing on:\n\
           - Performance problems\n\
           - Security concerns\n\
           - Code maintainability\n\
           - Rust best practices\n\
        \n\
        Format the response with a '## Summary' section followed by a '## Feedback' section with a markdown list.\n\
        Only provide feedback if there are concrete issues to address.\n\
        If the patch lacks sufficient context to make meaningful suggestions, indicate which additional files or \
        information would be helpful to review in a '## Additional Context Needed' section.\n\n\
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

    trace!("Request: {:?}", request);
    trace!("Response: {:?}", response);

    let review = response["content"][0]["text"]
        .as_str()
        .ok_or("Failed to get response text")?
        .to_string();

    Ok(review)
}

async fn analyze_patch(patch: &str, output: &mut OutputBuffer) -> Result<(), Box<dyn Error>> {
    let additions = patch.lines().filter(|l| l.starts_with('+')).count();
    let deletions = patch.lines().filter(|l| l.starts_with('-')).count();

    output.add_box_content(&format!(
        "Changed {} lines ({} additions, {} deletions)",
        additions + deletions,
        additions,
        deletions
    ));

    // Get Claude's review
    if let Ok(review) = get_code_review(patch).await {
        // Split the review into sections
        let sections: Vec<&str> = review.split("## ").collect();

        for section in sections {
            if section.starts_with("Summary") {
                output.add_section("Change Summary");
                output.add_box_content(section.replace("Summary\n", "").trim());
            } else if section.starts_with("Feedback") {
                output.add_section("AI Suggestions");
                output.add_box_content(section.replace("Feedback\n", "").trim());
            } else if section.starts_with("Additional Context Needed") {
                output.add_section("Additional Context Needed");
                output.add_box_content(section.replace("Additional Context Needed\n", "").trim());
            }
        }
    }

    // Prepare to collect feedback
    let mut feedback: Vec<String> = Vec::new();

    // Basic code hygiene
    if patch.contains("TODO") || patch.contains("FIXME") {
        feedback.push("Outstanding TODOs/FIXMEs should be addressed before merging".to_string());
    }

    if patch.contains("println!") || patch.contains("dbg!") {
        feedback.push("Remove debug print statements before merging".to_string());
    }

    // Error handling patterns
    if patch.contains("unwrap()") {
        feedback.push("Replace unwrap() calls with proper error handling".to_string());
    }

    if patch.contains("expect(") {
        feedback.push("Consider replacing expect() with more graceful error handling".to_string());
    }

    if patch.contains("panic!") {
        feedback.push(
            "Consider replacing panic! with Result/Option for graceful error handling".to_string(),
        );
    }

    // Memory and performance patterns
    if patch.contains("Clone") || patch.contains("clone()") {
        feedback
            .push("Review clone() usage - consider using references where possible".to_string());
    }

    if patch.contains("Box::new") {
        feedback.push("Verify if heap allocation via Box is necessary".to_string());
    }

    if patch.contains("Vec::new()") && !patch.contains("with_capacity") {
        feedback.push("Consider using Vec::with_capacity() if the size is known".to_string());
    }

    // Concurrency and async patterns
    if patch.contains("Mutex") && !patch.contains("RwLock") {
        feedback.push("Consider if RwLock would be more appropriate than Mutex".to_string());
    }

    if patch.contains(".await") && patch.contains("Vec") {
        feedback.push("Review concurrent operations on Vec - consider using join_all() for parallel execution".to_string());
    }

    // Security considerations
    if patch.contains("unsafe") {
        feedback
            .push("Unsafe block detected - ensure safety guarantees are documented".to_string());
    }

    if patch.contains("as_ptr") || patch.contains("as_mut_ptr") {
        feedback.push("Raw pointer usage detected - verify memory safety".to_string());
    }

    // Testing patterns
    let has_new_fn = patch
        .lines()
        .any(|l| l.contains("fn ") && !l.contains("test"));
    let has_test = patch.contains("#[test]");
    if has_new_fn && !has_test {
        feedback.push("New functions added without corresponding tests".to_string());
    }

    // Display feedback if any exists
    if !feedback.is_empty() {
        output.add_section("AI Suggestions");
        output.add_box_content(&feedback.join("\n"));
    }

    Ok(())
}

fn display_comments(comments: &[Comment], output: &mut OutputBuffer) {
    if comments.is_empty() {
        output.add_box_content("No comments found for this PR.");
    } else {
        for comment in comments {
            output.add_section(&format!(
                "Author: {} (at {})",
                comment.user.login, comment.created_at
            ));
            output.add_box_content(&comment.body);
        }
    }
}

fn get_pr_details(
    pr_number: u32,
    owner: &str,
    repo: &str,
    github_token: Option<&str>,
) -> Result<(PullRequestDetail, Vec<Comment>), Box<dyn Error>> {
    info!("Downloading PR #{} details...", pr_number);

    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}",
        owner, repo, pr_number
    );

    let mut request = ureq::get(&url).set("User-Agent", "rubber");
    if let Some(token) = github_token {
        request = request.set("Authorization", &format!("Bearer {}", token));
    }

    let response = request.call()?.into_string()?;
    let mut details: PullRequestDetail = serde_json::from_str(&response)?;

    // Fetch files data from a different endpoint
    info!("Downloading PR file changes...");

    let files_url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}/files",
        owner, repo, pr_number
    );

    let mut files_request = ureq::get(&files_url).set("User-Agent", "rubber");
    if let Some(token) = github_token {
        files_request = files_request.set("Authorization", &format!("Bearer {}", token));
    }

    let files_response = files_request.call()?.into_string()?;
    let files: Vec<FileChange> = serde_json::from_str(&files_response)?;
    details.files = files;

    // Get comments
    info!("Downloading PR comments...");
    let comments = get_pr_comments(&details.comments_url, github_token)?;

    Ok((details, comments))
}

async fn display_pr_details(
    details: &PullRequestDetail,
    comments: &[Comment],
    output: &mut OutputBuffer,
) -> Result<(), Box<dyn Error>> {
    // Title header
    output.add_header(&details.title);

    // Description section
    output.add_section("Description");
    if let Some(body) = &details.body {
        if !body.trim().is_empty() {
            output.add_box_content(body);
        } else {
            output.add_box_content("No description provided.");
        }
    } else {
        output.add_box_content("No description provided.");
    }

    // Files section
    output.add_section("Modified Files");

    if details.files.is_empty() {
        output.add_box_content("No files modified in this PR.");
    } else {
        // File summary table
        output.add_line(&format!(
            "┃  {:<50} {:<10} {:<10} {:<10}",
            "Filename", "Status", "Additions", "Deletions"
        ));
        output.add_line(&format!("┃  {}", "─".repeat(80)));

        let mut first = true;
        for file in &details.files {
            output.add_line(&format!(
                "┃  {:<50} {:<10} {:<10} {:<10}",
                file.filename, file.status, file.additions, file.deletions
            ));
        }
        output.add_diff_separator();

        for file in &details.files {
            if let Some(patch) = &file.patch {
                if !first {
                    output.add_diff_separator();
                }
                first = false;

                output.add_diff_header(&file.filename);
                output.add_diff_content(patch);

                // Add info message before analysis
                info!("Analyzing changes in {}...", file.filename);

                // Analysis section for this file
                output.add_section("Static Analysis");
                analyze_patch(patch, output).await?;
            }
        }
    }

    output.add_diff_separator();
    output.add_line("");

    // Comments section
    output.add_header("Comments");
    display_comments(comments, output);

    output.add_diff_separator();
    output.add_line("");

    Ok(())
}

fn find_pr_by_number(prs: &[PullRequest], number: u32) -> Option<&PullRequest> {
    prs.iter().find(|pr| pr.number == number)
}

impl Default for FileChange {
    fn default() -> Self {
        Self {
            filename: String::new(),
            status: String::new(),
            additions: 0,
            deletions: 0,
            changes: 0,
            patch: None,
        }
    }
}

impl Default for PullRequestDetail {
    fn default() -> Self {
        Self {
            title: String::new(),
            body: None,
            html_url: String::new(),
            user: User {
                login: String::new(),
            },
            created_at: String::new(),
            comments_url: String::new(),
            files: Vec::new(),
        }
    }
}

async fn run() -> Result<String, Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    let github_token = env::var("GITHUB_TOKEN").ok();

    let mut output = OutputBuffer::new();
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        error!("Usage: {} <owner> <repo> [pr_number]", args[0]);
        std::process::exit(1);
    }

    let owner = &args[1];
    let repo = &args[2];

    // Before fetching PR list
    info!("Fetching recent PRs for {}/{}...", owner, repo);

    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls?state=all&sort=created&direction=desc&per_page=10",
        owner, repo
    );

    // If PR number is provided, show its details directly
    if let Some(pr_number) = args.get(3) {
        match pr_number.parse::<u32>() {
            Ok(number) => match get_pr_details(number, owner, repo, github_token.as_deref()) {
                Ok((details, comments)) => {
                    display_pr_details(&details, &comments, &mut output).await?;
                    return Ok(output.content);
                }
                Err(e) => {
                    error!("Error fetching PR details: {}", e);
                    return Ok("Error fetching PR details.".to_string());
                }
            },
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

    let mut request = ureq::get(&url).set("User-Agent", "rubbery");
    if let Some(token) = &github_token {
        request = request.set("Authorization", &format!("Bearer {}", token));
    }

    let response = request.call()?.into_json::<Vec<PullRequest>>()?;

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
            let comments_count = match get_comments_count(&pr.comments_url, github_token.as_deref())
            {
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
                    if let Some(_pr) = find_pr_by_number(&response, pr_number) {
                        match get_pr_details(pr_number, owner, repo, github_token.as_deref()) {
                            Ok((details, comments)) => {
                                display_pr_details(&details, &comments, &mut output).await?;
                                return Ok(output.content);
                            }
                            Err(e) => {
                                error!("Error fetching PR details: {}", e);
                                return Ok("Error fetching PR details.".to_string());
                            }
                        }
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
