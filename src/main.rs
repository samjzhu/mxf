use std::{env, fs};
use std::ffi::OsString;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::process::Command;
use actix_web::{web, get, App, Responder, HttpResponse, HttpServer, middleware, HttpRequest, Error};
use actix_web::web::{Json};
use actix_multipart::form::{
    tempfile::{TempFile},
    MultipartForm,
};
use base64::{Engine as _, engine::{general_purpose}};
use qrcode_generator::QrCodeEcc;

use lazy_static::lazy_static; // new line
use tera::{ Tera, Context }; // new line
use local_ip_address::local_ip;
use serde::{Serialize};

use env_logger;
use log::info;

use include_dir::{include_dir};
static STATIC_DIR: include_dir::Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/static");

lazy_static!{
  pub static ref TEMPLATES: Tera = {
    let mut tera = match Tera::new("templates/**/*.html") {
      Ok(t) => t,
      Err(e) => {
        println!("Parsing error(s): {}", e);
        std::process::exit(1);
      }
    };
    let home_temp_str = include_str!("../templates/home.html");
        let _ = tera.add_raw_template("home.html", home_temp_str);
        tera.autoescape_on(vec![".html", ".sql"]);
        tera
  };
}

struct MyConfig{
    working_dir: String,
    protocol: String,
    port: String,
}

impl MyConfig {
    pub fn from_env() -> Self {
        Self {
            working_dir: env::var("working_dir").unwrap_or(env::current_dir().unwrap().display().to_string()),
            protocol: env::var("protocol").unwrap_or("http".to_string()),
            port: env::var("PORT").unwrap_or("8000".to_string()),
        }
    }
}

#[derive(Serialize)]
struct FileItem {
    url: String,
    qr: String,
    name:String,
}


#[derive(Debug, MultipartForm)]
struct UploadForm {
    #[multipart(rename = "file")]
    files: Vec<TempFile>,
}

#[derive(Serialize)]
struct FileUploaded {
    name: String,
    url: String,
}

async fn list_files() -> HttpResponse{
    let mut context = Context::new();
    let working_dir = env::var("working_dir").unwrap_or(env::current_dir().unwrap().display().to_string());

    let paths = fs::read_dir(working_dir).unwrap();
    let mut file_list:Vec<FileItem> = vec![];
    for path in paths {
        let dir = path.unwrap();
        if dir.file_type().unwrap().is_file() {
            let file_url = saved_file_url(dir.file_name());
            let f_name = dir.file_name();
            // let qr_b64 = generate_qr_n64(file_url.clone());
            // let fi = FileItem { url: file_url, qr: qr_b64 };
            let fi = FileItem { url: file_url, qr: "void".parse().unwrap(), name:f_name.into_string().unwrap() };
            file_list.push(fi);
        }
    }
    context.insert("file_list", &file_list);

    let ip = local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let run_parm = MyConfig::from_env();
    let host_url =   format!("{}://{}:{}",
                             run_parm.protocol,
                             ip,
                             run_parm.port
    );

    let qr_b64 = generate_qr_n64(host_url.clone());
    context.insert("host_url_qr", &qr_b64);
    context.insert("host_url", &host_url);
    let template = TEMPLATES.render("home.html", &context).expect("Error");
    HttpResponse::Ok().body(template)
}
#[get("/share/{name}")]
pub async fn download_file(req :HttpRequest, path_name :web::Path<String>) -> HttpResponse {
    let config = MyConfig::from_env();
    let path = Path::new(&config.working_dir).join(path_name.into_inner());
    info!("downloading file: {}", path.display());
    let file = actix_files::NamedFile::open_async(path).await.unwrap();
    file.into_response(&req)
}

#[get("/static/{name}")]
pub async fn static_file(_req :HttpRequest, path_name :web::Path<String>) -> HttpResponse {
    let file = STATIC_DIR.get_file(path_name.to_string()).unwrap();
    let file_body = file.contents_utf8().unwrap();
    HttpResponse::Ok().body(file_body)
}

fn saved_file_url(file_name: OsString) -> String {
    let config = MyConfig::from_env();
    let ip = local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    format!(
        "{}://{}:{}/{}/{}",
        config.protocol,
        ip,
        config.port,
        "share",
        file_name.to_str().unwrap()
    )
}
async fn save_files(
    MultipartForm(form): MultipartForm<UploadForm>,
) -> Result<impl Responder, Error> {
    Ok(Json(saving_files(form.files)))
}

fn saving_files(files: Vec<TempFile>) -> Vec<FileUploaded> {
    let config = MyConfig::from_env();
    let mut file_name_list = Vec::new();
    for f in files {
        let f_name = f.file_name.unwrap();
        let path = format!("{}/{}", config.working_dir, f_name);
        let url = saved_file_url(f_name.to_owned().into());
        let saved_file = FileUploaded { name: f_name, url };
        file_name_list.push(saved_file);
        info!("saving to {path}");
        f.file.persist(path).unwrap();
    }
    return file_name_list;
}

pub fn generate_qr_n64(text: String) -> String {
    let image_bytes = qrcode_generator::to_png_to_vec(text, QrCodeEcc::Low, 1024).unwrap();
    general_purpose::STANDARD.encode(image_bytes)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    let ip = local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let run_parm = MyConfig::from_env();
    let host_url =   format!("{}://{}:{}",
                             run_parm.protocol,
                             ip,
                             run_parm.port
    );

    info!(
        "starting HTTP server at {}", host_url
        );
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C","start", "msedge", &host_url])
            .output()
            .expect("Failed to start firefox")

    } else {
        Command::new("sh")
            .arg("-c")
            .arg("firefox")
            .arg(&host_url)
            .output()
            .expect("failed to execute process")
    };
    output.stdout;
    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .service(web::resource("/").route(web::get().to(list_files)))
            .service(web::resource("/list").route(web::get().to(list_files)))
            .service(web::resource("/upload").route(web::post().to(save_files)))
            .service(download_file)
            .service(static_file)
            .service(web::resource("/api/upload").route(web::post().to(save_files)))

    })
        .bind(("0.0.0.0", MyConfig::from_env().port.parse::<u16>().unwrap(),))?
        .run()
        .await
}



