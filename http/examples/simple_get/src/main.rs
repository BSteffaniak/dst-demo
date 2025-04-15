use dst_demo_http::GenericClient;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] dst_demo_http::Error),
    #[error("MissingUrlArgument")]
    MissingUrlArgument,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let Some(url) = std::env::args().nth(1) else {
        return Err(Error::MissingUrlArgument);
    };

    log::info!("args={url:?}");

    let response = dst_demo_http::Client::new().get(&url).send().await?;

    println!("response: {}", response.text().await?);

    Ok(())
}
