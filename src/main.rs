#[macro_use]
extern crate prettytable;

mod db;
mod job;

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Read},
    net::{TcpListener, TcpStream},
    str::FromStr,
};

use chrono::{Local, NaiveDate};
use clap::{Args, Parser, Subcommand};
use db::DB;
use prettytable::{csv, Cell, Row, Table};
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct App {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Add a new job with title, description, and an optional date")]
    Add {
        title: String,
        description: String,
        date: Option<String>,
    },
    #[command(about = "List all jobs")]
    List,
    #[command(about = "Search for jobs, optionally filtering by title, description, and date")]
    Search(SearchArgs),
    #[command(about = "Remove a job by its id")]
    Remove { id: i32 },
    #[command(about = "Visualize jobs in a web browser.")]
    Serve,
    #[command(about = "Export jobs to a file.")]
    Export(ExportArgs),
    #[command(about = "Clear the database")]
    Clear,
}

#[derive(Args, Debug)]
struct ExportArgs {
    #[arg(long, default_value = "jobs.json", help = "File to export jobs to.")]
    file: String,
    #[arg(
        long,
        default_value = "json",
        help = "Format of the exported file. Options: json, csv"
    )]
    format: Format,
}

#[derive(Debug, Clone)]
enum Format {
    Json = 1,
    Csv = 2,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            _ => Err(format!("\nInvalid format: {}. Valid formats: json, csv", s)),
        }
    }
}

#[derive(Debug, Args)]
struct SearchArgs {
    #[arg(short, long, default_value = "")]
    title: Option<String>,
    #[arg(short, long, default_value = "")]
    description: Option<String>,
    #[arg(long, default_value = "")]
    date: Option<String>,
}

fn serve_jobs(queries: &db::Queries) {
    let tcp = TcpListener::bind("127.0.0.1:8080")
        .map_err(|e| eprintln!("Error while binding to port: {}", e))
        .unwrap();

    println!("Listening on http://127.0.0.1:8080");
    for stream in tcp.incoming() {
        match stream {
            Ok(stream) => handle_request(stream, queries),
            Err(e) => eprintln!("Error while accepting connection: {}", e),
        }
    }
}

fn handle_request(mut stream: TcpStream, queries: &db::Queries) {
    let mut buf_reader = BufReader::new(&stream);
    let mut content_length = 0;
    let mut lines = Vec::new();

    // get the content length and the headers
    // i don't need them in a hashmap, so i'll just store them as a string
    for line_result in buf_reader.by_ref().lines() {
        match line_result {
            Ok(line) => {
                if line.is_empty() {
                    break;
                }
                if line.starts_with("Content-Length: ") {
                    content_length = line
                        .split("Content-Length: ")
                        .last()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                }
                lines.push(line);
            }
            Err(e) => eprintln!("Error while reading request line: {}", e),
        }
    }

    let request_line = lines.join("\n");

    // read the body if there is one
    let mut body = String::new();
    if content_length > 0 {
        buf_reader
            .take(content_length as u64)
            .read_to_string(&mut body)
            .expect("Failed to read request body");
    }

    if request_line.starts_with("GET / HTTP/1.1") {
        let jobs = queries.list_jobs();
        let mut job_list = String::new();

        for job in jobs {
            let date = NaiveDate::parse_from_str(&job.date, "%d-%m-%Y").unwrap();
            let delete_button = format!(
                r#"<form action="/delete/{}" method="POST" class='delete-btn-form'>
                    <button type="submit">Delete</button>
                </form>"#,
                job.id
            );
            job_list.push_str("<tr>");
            job_list.push_str(&format!("<td>{}</td>", job.id));
            job_list.push_str(&format!("<td>{}</td>", job.title));
            job_list.push_str(&format!("<td>{}</td>", job.description));
            job_list.push_str(&format!("<td>{}</td>", date.format("%d/%m/%Y").to_string()));
            job_list.push_str(&format!("<td>{}</td>", delete_button));
            job_list.push_str("</tr>");
        }

        let html = include_str!("../static/index.html");
        let content = String::from(html.replace("{content}", &job_list));

        let status_line = "HTTP/1.1 200 OK";
        let response = format!(
            "{}\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
            status_line,
            content.len(),
            content
        );
        stream.write_all(response.as_bytes()).unwrap();
    } else if request_line.starts_with("GET /styles.css HTTP/1.1") {
        let contents = include_str!("../static/styles.css");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\n\r\n{}",
            contents.len(),
            contents
        );
        stream.write_all(response.as_bytes()).unwrap();
    } else if request_line.starts_with("GET /create HTTP/1.1") {
        let contents = include_str!("../static/create.html");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
            contents.len(),
            contents
        );
        stream.write_all(response.as_bytes()).unwrap();
    } else if request_line.starts_with("POST /create_job HTTP/1.1") {
        let b: HashMap<String, String> = serde_json::from_str(&body).unwrap();

        let title = b.get("title").unwrap();
        let description = b.get("description").unwrap();

        let date = format_date("today".to_string());

        queries.add_job(title.to_string(), description.to_string(), date);

        let r = format!("Job created successfully");

        // Respond with 200 OK
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            r.len(),
            r
        );
        stream.write_all(response.as_bytes()).unwrap();
    } else if request_line.starts_with("POST /delete/") {
        let first_line = lines.first().unwrap();
        let id = first_line.split("POST /delete/").last().unwrap();
        let id = id.split(" HTTP/1.1").next().unwrap().trim();
        queries.remove_job(id.parse::<i32>().unwrap());
        let response = "HTTP/1.1 303 See Other\r\nLocation: /\r\n\r\n";
        stream.write_all(response.as_bytes()).unwrap();
    } else if request_line.starts_with("POST /export HTTP/1.1") {
        let b: HashMap<String, String> = serde_json::from_str(&body).unwrap();
        let format = b.get("format").unwrap();
        let content_type: String;
        if format == "json" {
            content_type = "application/json".to_string();
        } else if format == "csv" {
            content_type = "text/csv".to_string();
        } else {
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/text\r\nContent-Length: {}\r\n\r\n{}",
                "Invalid format".len(),
                "Invalid format"
            );
            stream.write_all(response.as_bytes()).unwrap();
            return;
        }

        let jobs = queries.list_jobs();
        let format = Format::from_str(format).map_err(|e| {
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/text\r\nContent-Length: {}\r\n\r\n{}",
                e.len(),
                e
            );
            stream.write_all(response.as_bytes()).unwrap();
            return;
        }).unwrap();

        let buf = export_jobs_to_bytes(jobs, format);

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
            content_type,
            buf.len(),
            String::from_utf8(buf).unwrap()
        );
        stream.write_all(response.as_bytes()).unwrap();
    } else {
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/text\r\nContent-Length: {}\r\n\r\n{}",
            "404 Not Found".len(),
            "404 Not Found"
        );
        stream.write_all(response.as_bytes()).unwrap();
    }
}

fn display_jobs(jobs: Vec<job::Job>) {
    let mut table = Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(row!["ID", "Title", "Description", "Date"]);

    for job in jobs {
        table.add_row(Row::new(vec![
            Cell::new(&job.id.to_string()),
            Cell::new(&job.title),
            Cell::new(&job.description),
            Cell::new(&job.date),
        ]));
    }
    table.printstd();
}

fn format_date(date: String) -> String {
    if date == "today" {
        return Local::now().format("%d-%m-%Y").to_string();
    } else {
        let formatted_date = match NaiveDate::parse_from_str(&date, "%d-%m-%Y") {
            Ok(date) => date.format("%d-%m-%Y").to_string(),
            Err(_) => "".to_string(),
        };
        if formatted_date.is_empty() {
            panic!("Invalid date format. Please use the format dd-mm-yyyy.");
        }
        return formatted_date;
    }
}

fn export_jobs(jobs: Vec<job::Job>, format: Format, file: &str) {
    match format {
        Format::Json => {
            let f = File::create(file)
                .map_err(|e| eprintln!("Error opening file: {}", e))
                .expect("Error opening file");
            let mut writer = BufWriter::new(f);

            writeln!(writer, "[").expect("Error writing to file");

            for (i, job) in jobs.iter().enumerate() {
                let job_json = format!(
                    r#"{{"id": {}, "title": "{}", "description": "{}", "date": "{}"}}"#,
                    job.id, job.title, job.description, job.date
                );

                if i < jobs.len() - 1 {
                    writeln!(writer, "{},", job_json).expect("Error writing job to file");
                } else {
                    writeln!(writer, "{}", job_json).expect("Error writing job to file");
                }
            }

            writeln!(writer, "]").expect("Error closing JSON array");
        }
        Format::Csv => {
            let headers = vec!["id", "title", "description", "date"];
            let mut wtr = csv::Writer::from_path(file)
                .map_err(|e| eprintln!("Error opening file: {}", e))
                .unwrap();
            wtr.write_record(headers)
                .map_err(|e| eprintln!("Error writing headers: {}", e))
                .unwrap();
            for job in jobs {
                let row = vec![job.id.to_string(), job.title, job.description, job.date];
                wtr.write_record(row)
                    .map_err(|e| eprintln!("Error writing row: {}", e))
                    .unwrap();
            }
        }
    }
    println!("Jobs exported successfully to {}", file);
}

fn export_jobs_to_bytes(jobs: Vec<job::Job>, format: Format) -> Vec<u8> {
    let mut buf = Vec::new();

    match format {
        Format::Json => {
            let mut wtr = BufWriter::new(&mut buf);
            writeln!(wtr, "[").expect("Error writing to file");
            for (i, job) in jobs.iter().enumerate() {
                let job_json = format!(
                    r#"{{"id": {}, "title": "{}", "description": "{}", "date": "{}"}}"#,
                    job.id, job.title, job.description, job.date
                );

                if i < jobs.len() - 1 {
                    writeln!(wtr, "{},", job_json).expect("Error writing job to file");
                } else {
                    writeln!(wtr, "{}", job_json).expect("Error writing job to file");
                }
            }
            writeln!(wtr, "]").expect("Error closing JSON array");
        }
        Format::Csv => {
            let headers = vec!["id", "title", "description", "date"];
            let mut wtr = csv::Writer::from_writer(&mut buf);
            wtr.write_record(headers)
                .map_err(|e| eprintln!("Error writing headers: {}", e))
                .unwrap();
            for job in jobs {
                let row = vec![job.id.to_string(), job.title, job.description, job.date];
                wtr.write_record(row)
                    .map_err(|e| eprintln!("Error writing row: {}", e))
                    .unwrap();
            }
        }
    }
    buf
}

fn main() {
    let db = DB::new();
    let conn = db.get_conn();
    let queries = db::Queries::new(conn);
    let args = App::parse();
    match args.cmd {
        Commands::Add {
            title,
            description,
            date,
        } => {
            let d: String;

            if date.is_none() {
                d = format_date("today".to_string());
            } else {
                d = format_date(date.unwrap());
            }

            queries.add_job(title, description, d);
            println!("Job added successfully");
        }
        Commands::Search(args) => {
            let jobs = queries.seach_jobs(args.title, args.description, args.date);
            display_jobs(jobs);
        }
        Commands::List => {
            let jobs = queries.list_jobs();
            display_jobs(jobs);
        }
        Commands::Remove { id } => {
            queries.remove_job(id);
            println!("Job removed successfully");
        }
        Commands::Serve => {
            serve_jobs(&queries);
        }
        Commands::Export(args) => {
            let jobs = queries.list_jobs();
            export_jobs(jobs, args.format, args.file.as_str());
        }
        Commands::Clear => {
            db.drop_db();
            println!("Database cleared successfully");
        }
    }

    db.close();
}
