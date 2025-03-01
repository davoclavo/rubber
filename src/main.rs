use serde::Deserialize;
use std::env;
use std::error::Error;

#[derive(Deserialize, Debug)]
struct PullRequest {
    number: u32,
    title: String,
    user: User,
    created_at: String,
    html_url: String,
}

#[derive(Deserialize, Debug)]
struct User {
    login: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <owner> <repo>", args[0]);
        std::process::exit(1);
    }

    let owner = &args[1];
    let repo = &args[2];

    println!("Fetching the 10 most recent PRs for {}/{}", owner, repo);

    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls?state=all&sort=created&direction=desc&per_page=10",
        owner, repo
    );

    let response = ureq::get(&url)
        .set("User-Agent", "github-pr-lister")
        .call()?
        .into_json::<Vec<PullRequest>>()?;

    if response.is_empty() {
        println!("No pull requests found.");
    } else {
        println!(
            "{:<6} {:<50} {:<20} {:<30}",
            "PR#", "Title", "Author", "Created At"
        );
        println!("{}", "-".repeat(106));

        for pr in response {
            // Truncate title if too long
            let title = if pr.title.len() > 47 {
                format!("{}...", &pr.title[..44])
            } else {
                pr.title
            };

            println!(
                "{:<6} {:<50} {:<20} {:<30}",
                pr.number, title, pr.user.login, pr.created_at
            );

            // Print the PR URL on a separate line
            println!("       URL: {}", pr.html_url);
        }
    }

    Ok(())
}
