use axum::{
    extract::Request,
    http::{self, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use base64::{Engine as _, engine::general_purpose};
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use uuid::Uuid;

// JWKS cache with TTL
struct JwksCache {
    keys: HashMap<String, (String, Instant)>,  // Store PEM string instead of DecodingKey
    ttl: Duration,
}

impl JwksCache {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
            ttl: Duration::from_secs(3600), // Cache for 1 hour
        }
    }

    fn get(&self, kid: &str) -> Option<String> {
        self.keys.get(kid).and_then(|(pem, inserted_at)| {
            if inserted_at.elapsed() < self.ttl {
                Some(pem.clone())
            } else {
                None
            }
        })
    }

    fn insert(&mut self, kid: String, pem: String) {
        self.keys.insert(kid, (pem, Instant::now()));
    }
}

lazy_static::lazy_static! {
    static ref JWKS_CACHE: Arc<RwLock<JwksCache>> = Arc::new(RwLock::new(JwksCache::new()));
}

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Jwk {
    kty: String,
    kid: String,
    #[serde(rename = "use")]
    use_: Option<String>,
    alg: Option<String>,
    crv: Option<String>,
    x: Option<String>,
    y: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

async fn fetch_jwks_key(supabase_url: &str, kid: &str) -> Result<DecodingKey, AuthError> {
    // Check cache first
    {
       let cache = JWKS_CACHE.read().unwrap();
        if let Some(pem) = cache.get(kid) {
            tracing::trace!("JWKS key found in cache for kid: {}", kid);
            return DecodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| {
                tracing::error!("Failed to create decoding key from cached PEM: {:?}", e);
                AuthError::ConfigError
            });
        }
    }
    
    tracing::info!("Fetching JWKS from: {}/auth/v1/.well-known/jwks.json", supabase_url);
    
    let jwks_url = format!("{}/auth/v1/.well-known/jwks.json", supabase_url);
    
    let response = reqwest::get(&jwks_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch JWKS: {:?}", e);
            AuthError::ConfigError
        })?;
    
    let jwks: JwksResponse = response.json().await.map_err(|e| {
        tracing::error!("Failed to parse JWKS: {:?}", e);
        AuthError::ConfigError
    })?;
    
    let key = jwks.keys.iter()
        .find(|k| k.kid == kid)
        .ok_or_else(|| {
            tracing::error!("Key ID {} not found in JWKS", kid);
            AuthError::InvalidToken
        })?;
    
    // Convert JWK to DecodingKey
    if key.kty == "EC" && key.crv.as_deref() == Some("P-256") {
        // For P-256 EC keys, construct PEM from x and y coordinates
        let x = key.x.as_ref().ok_or(AuthError::ConfigError)?;
        let y = key.y.as_ref().ok_or(AuthError::ConfigError)?;
        
        // Decode base64url coordinates
        let x_bytes = general_purpose::URL_SAFE_NO_PAD.decode(x).map_err(|e| {
            tracing::error!("Failed to decode x coordinate: {:?}", e);
            AuthError::ConfigError
        })?;
        let y_bytes = general_purpose::URL_SAFE_NO_PAD.decode(y).map_err(|e| {
            tracing::error!("Failed to decode y coordinate: {:?}", e);
            AuthError::ConfigError
        })?;
        
        // Build uncompressed EC point (0x04 prefix + x + y)
        let mut point = vec![0x04];
        point.extend_from_slice(&x_bytes);
        point.extend_from_slice(&y_bytes);
        
        // P-256 OID: 1.2.840.10045.3.1.7
        let oid = vec![0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
        
        // Build SubjectPublicKeyInfo structure
        let mut spki = vec![
            0x30, 0x59, // SEQUENCE, length 89
            0x30, 0x13, // SEQUENCE, length 19 (algorithm)
            0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, // OID ecPublicKey
        ];
        spki.extend_from_slice(&oid);
        spki.push(0x03); // BIT STRING
        spki.push(0x42); // length 66
        spki.push(0x00); // no unused bits
        spki.extend_from_slice(&point);
        
        // Encode as base64
        let b64 = general_purpose::STANDARD.encode(&spki);
        
        // Create PEM
        let pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
            b64.chars()
                .collect::<Vec<_>>()
                .chunks(64)
                .map(|c| c.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        );
        
        let decoding_key = DecodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| {
            tracing::error!("Failed to create decoding key from PEM: {:?}", e);
            AuthError::ConfigError
        })?;
        
        // Cache the PEM string
        {
            let mut cache = JWKS_CACHE.write().unwrap();
            cache.insert(kid.to_string(), pem);
        }
        
        Ok(decoding_key)
    } else {
        tracing::error!("Unsupported key type or curve: {} {:?}", key.kty, key.crv);
        Err(AuthError::ConfigError)
    }
}

/// Claims extracted from Supabase JWT
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,  // User ID
    pub email: Option<String>,
    pub role: Option<String>,
    pub exp: usize,
    pub iat: usize,
}

/// Extension type to pass user info through handlers
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    #[allow(dead_code)]
    pub email: Option<String>,
}

/// Extract user from Authorization header and validate JWT
pub async fn auth_middleware(
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Extract token from Authorization header
    let auth_header = request
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidToken)?;

    // Get Supabase JWT secret from env
    let jwt_secret = env::var("SUPABASE_JWT_SECRET")
        .map_err(|_| AuthError::ConfigError)?;

    // Check which algorithm the token uses
    let header = decode_header(token).map_err(|e| {
        tracing::warn!("Failed to decode JWT header: {:?}", e);
        AuthError::InvalidToken
    })?;

    let token_data = match header.alg {
        Algorithm::HS256 => {
            // Legacy Supabase secret (HS256)
            let mut validation = Validation::new(Algorithm::HS256);
            validation.validate_exp = true;
            validation.set_audience(&["authenticated"]); // Supabase uses 'authenticated' as audience
            validation.validate_aud = false; // But disable it for now to be flexible
            
            decode::<Claims>(
                token,
                &DecodingKey::from_secret(jwt_secret.as_bytes()),
                &validation,
            )
        }
        Algorithm::ES256 => {
            // New Supabase ECC keys (ES256) - fetch from JWKS
            let supabase_url = env::var("SUPABASE_URL")
                .or_else(|_| {
                    // Extract project ref from DATABASE_URL
                    // Format: postgresql://postgres.PROJECT_REF:pass@aws-x-region.pooler.supabase.com:5432/db
                    env::var("DATABASE_URL").and_then(|url| {
                        url.split("postgres.").nth(1)
                            .and_then(|s| s.split(':').next())
                            .map(|project_ref| format!("https://{}.supabase.co", project_ref))
                            .ok_or_else(|| std::env::VarError::NotPresent)
                    })
                })
                .map_err(|_| {
                    tracing::error!("SUPABASE_URL not set and couldn't extract from DATABASE_URL");
                    AuthError::ConfigError
                })?;
            
            let kid = header.kid.as_ref().ok_or_else(|| {
                tracing::warn!("ES256 token missing kid header");
                AuthError::InvalidToken
            })?;
            
            let decoding_key = fetch_jwks_key(&supabase_url, kid).await?;
            
            let mut validation = Validation::new(Algorithm::ES256);
            validation.validate_exp = true;
            validation.validate_aud = false; // Disable audience validation
            
            decode::<Claims>(token, &decoding_key, &validation)
        }
        _ => {
            tracing::warn!("Unsupported algorithm: {:?}", header.alg);
            return Err(AuthError::InvalidToken);
        }
    }
    .map_err(|e| {
        tracing::warn!("JWT validation failed: {:?}", e);
        AuthError::InvalidToken
    })?;

    let claims = token_data.claims;
    
    // Parse user_id as UUID
    let user_id = Uuid::parse_str(&claims.sub).map_err(|e| {
        tracing::error!("Invalid user ID format in JWT: {:?}", e);
        AuthError::InvalidToken
    })?;
    
    tracing::trace!(
        "Authenticated user: {} ({})", 
        user_id, 
        claims.email.as_deref().unwrap_or("no-email")
    );

    // Insert user info into request extensions
    let auth_user = AuthUser {
        user_id,
        email: claims.email.clone(),
    };
    
    request.extensions_mut().insert(auth_user);

    Ok(next.run(request).await)
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    ConfigError,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authorization token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid or expired token"),
            AuthError::ConfigError => (StatusCode::INTERNAL_SERVER_ERROR, "Authentication configuration error"),
        };

        let body = Json(json!({
            "error": message
        }));

        (status, body).into_response()
    }
}
