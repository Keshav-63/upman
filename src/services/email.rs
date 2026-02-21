use std::env;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use lettre::transport::smtp::authentication::Credentials;

/// Send an email alert using SMTP
pub async fn send_email(to: &str, subject: &str, body: &str) {
    let smtp_host = match env::var("SMTP_HOST") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("SMTP_HOST not set, skipping email");
            return;
        }
    };

    let smtp_port: u16 = match env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()) {
        Some(v) => v,
        None => {
            tracing::warn!("SMTP_PORT not set or invalid, skipping email");
            return;
        }
    };

    let smtp_user = match env::var("SMTP_USER") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("SMTP_USER not set, skipping email");
            return;
        }
    };

    let smtp_pass = match env::var("SMTP_PASS") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("SMTP_PASS not set, skipping email");
            return;
        }
    };

    let from = match env::var("ALERT_FROM") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("ALERT_FROM not set, skipping email");
            return;
        }
    };

    let email = Message::builder()
        .from(from.parse().unwrap())
        .to(to.parse().unwrap())
        .subject(subject)
        .body(body.to_string())
        .unwrap();

    let creds = Credentials::new(smtp_user, smtp_pass);

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_host)
        .unwrap()
        .port(smtp_port)
        .credentials(creds)
        .build();

    match mailer.send(email).await {
        Ok(_) => {
            tracing::info!("📧 Email sent successfully to {} | subject: {}", to, subject);
        }
        Err(e) => {
            tracing::error!("❌ Failed to send email to {}: {:?}", to, e);
        }
    }
}
