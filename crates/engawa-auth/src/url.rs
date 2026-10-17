//! The two URLs sign-in depends on: where Engawa is (its public URL) and who signs people in
//! (the issuer).
//!
//! Both must be `https`: the session cookie is `Secure` with the `__Host-` prefix, which browsers
//! accept only from a secure origin [rfc6265bis] (§4.1.3.2), and the client secret and tokens
//! travel to the issuer. Plain `http` is accepted only for a loopback host, which browsers treat
//! as potentially trustworthy [secure-contexts] (§3.1), for development and tests.
//!
//! [rfc6265bis]: https://www.ietf.org/archive/id/draft-ietf-httpbis-rfc6265bis-22.txt
//! [secure-contexts]: https://w3c.github.io/webappsec-secure-contexts/

use std::fmt;

/// Why a URL is not usable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlError {
    /// Not an absolute URL.
    #[error("{0:?} is not an absolute URL")]
    NotAbsolute(String),
    /// `http` to a host other than a loopback address, or another scheme.
    #[error("{0:?} must use https (http only for 127.0.0.1, [::1] or localhost)")]
    NotSecure(String),
    /// User name or password in the URL, a query or a fragment.
    #[error("{0:?} must not carry a user name, a query or a fragment")]
    Extra(String),
    /// The public URL has a path: Engawa is served at the root of its host.
    #[error("{0:?} must be the root of a host, like https://engawa.example.com")]
    Path(String),
}

/// A checked `https` URL (or `http` on loopback), without a trailing slash.
#[derive(Clone, PartialEq, Eq)]
pub struct BaseUrl {
    text: String,
    scheme: String,
    authority: String,
}

impl fmt::Debug for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl BaseUrl {
    /// Checks `text`; a path is kept (an issuer may have one), a trailing `/` dropped.
    ///
    /// # Errors
    ///
    /// When it is not absolute, not secure, or carries credentials, a query or a fragment.
    pub fn parse(text: &str) -> Result<Self, UrlError> {
        let owned = || text.to_owned();
        if text.contains('#') || text.contains('?') {
            return Err(UrlError::Extra(owned()));
        }
        let uri: http::Uri = text
            .parse()
            .map_err(|_invalid| UrlError::NotAbsolute(owned()))?;
        let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
            return Err(UrlError::NotAbsolute(owned()));
        };
        if authority.as_str().contains('@') {
            return Err(UrlError::Extra(owned()));
        }
        let loopback = matches!(authority.host(), "127.0.0.1" | "[::1]" | "localhost");
        match scheme {
            "https" => {}
            "http" if loopback => {}
            _ => return Err(UrlError::NotSecure(owned())),
        }
        // Browsers leave the default port out of `Host` and `Origin`.
        let default_port = match scheme {
            "https" => 443,
            _ => 80,
        };
        let authority = if authority.port_u16() == Some(default_port) {
            authority.host().to_ascii_lowercase()
        } else {
            authority.as_str().to_ascii_lowercase()
        };
        let path = uri.path().trim_end_matches('/');
        Ok(Self {
            text: format!("{scheme}://{authority}{path}"),
            scheme: scheme.to_owned(),
            authority,
        })
    }

    /// Checks `text` as Engawa's public URL: also refuses a path.
    ///
    /// # Errors
    ///
    /// As [`BaseUrl::parse`], and when it has a path.
    pub fn parse_root(text: &str) -> Result<Self, UrlError> {
        let url = Self::parse(text)?;
        if url.text == url.origin() {
            Ok(url)
        } else {
            Err(UrlError::Path(text.to_owned()))
        }
    }

    /// The URL, without a trailing slash.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// `scheme://host[:port]`, as browsers send it in `Origin`.
    #[must_use]
    pub fn origin(&self) -> String {
        format!("{}://{}", self.scheme, self.authority)
    }

    /// `host[:port]`, lower case, as browsers send it in `Host`.
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// This URL followed by `path` (which starts with `/`).
    #[must_use]
    pub fn join(&self, path: &str) -> String {
        format!("{}{path}", self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_urls_are_kept_without_a_trailing_slash() {
        let url = BaseUrl::parse("https://Genkan.Example.com/").unwrap();
        assert_eq!(url.as_str(), "https://genkan.example.com");
        assert_eq!(url.origin(), "https://genkan.example.com");
        let default_port = BaseUrl::parse("https://engawa.example.com:443").unwrap();
        assert_eq!(default_port.authority(), "engawa.example.com");
        let issuer = BaseUrl::parse("https://id.example.com:8443/genkan").unwrap();
        assert_eq!(issuer.as_str(), "https://id.example.com:8443/genkan");
        assert_eq!(issuer.authority(), "id.example.com:8443");
        assert_eq!(
            issuer.join("/.well-known/openid-configuration"),
            "https://id.example.com:8443/genkan/.well-known/openid-configuration"
        );
    }

    #[test]
    fn plain_http_is_refused_except_on_loopback() {
        assert!(matches!(
            BaseUrl::parse("http://engawa.example.com"),
            Err(UrlError::NotSecure(_))
        ));
        assert!(matches!(
            BaseUrl::parse("ftp://engawa.example.com"),
            Err(UrlError::NotSecure(_))
        ));
        assert!(BaseUrl::parse("http://127.0.0.1:8080").is_ok());
        assert!(BaseUrl::parse("http://[::1]:8080").is_ok());
    }

    #[test]
    fn credentials_queries_fragments_and_relative_urls_are_refused() {
        for bad in [
            "https://user@engawa.example.com",
            "https://engawa.example.com/?a=b",
            "https://engawa.example.com/#x",
        ] {
            assert!(
                matches!(BaseUrl::parse(bad), Err(UrlError::Extra(_))),
                "{bad}"
            );
        }
        assert!(matches!(
            BaseUrl::parse("/relative"),
            Err(UrlError::NotAbsolute(_))
        ));
    }

    #[test]
    fn the_public_url_is_the_root_of_a_host() {
        assert!(BaseUrl::parse_root("https://engawa.example.com/").is_ok());
        assert!(matches!(
            BaseUrl::parse_root("https://example.com/engawa"),
            Err(UrlError::Path(_))
        ));
    }
}
