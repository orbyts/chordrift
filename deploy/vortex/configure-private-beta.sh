#!/bin/sh

# Creates the host-only Chordrift environment without echoing secrets. This is
# intentionally interactive and must be run directly by the Vortex operator.

set -eu
umask 077

CONFIG_DIR=${XDG_CONFIG_HOME:-"$HOME/.config"}/chordrift-hosted
CONFIG_FILE=$CONFIG_DIR/chordrift.env
ACCOUNT_ID=669e8896-0ba1-f730-67cf-351467f4d6eb

mkdir -p "$CONFIG_DIR"
chmod 700 "$CONFIG_DIR"

printf 'Verified Google email used for the one-time existing-account claim: '
IFS= read -r BOOTSTRAP_EMAIL
printf 'Auth0 tenant domain (for example tenant.us.auth0.com): '
IFS= read -r AUTH0_DOMAIN
printf 'Auth0 client ID: '
IFS= read -r AUTH0_CLIENT_ID
printf 'Auth0 client secret: '
stty -echo
IFS= read -r AUTH0_CLIENT_SECRET
stty echo
printf '\nNeon connection URL for the intended chordrift project: '
stty -echo
IFS= read -r DATABASE_URL
stty echo
printf '\n'

case "$BOOTSTRAP_EMAIL$AUTH0_DOMAIN$AUTH0_CLIENT_ID$AUTH0_CLIENT_SECRET$DATABASE_URL" in
    *'
'*) printf 'Values must not contain newlines. Nothing was written.\n' >&2; exit 2 ;;
esac
case "$BOOTSTRAP_EMAIL" in *@*) ;; *) printf 'A valid bootstrap email is required.\n' >&2; exit 2 ;; esac
case "$AUTH0_DOMAIN" in https://*|http://*|*/*) printf 'Enter only the Auth0 hostname.\n' >&2; exit 2 ;; esac
case "$DATABASE_URL" in postgresql://*|postgres://*) ;; *) printf 'A PostgreSQL URL is required.\n' >&2; exit 2 ;; esac

TEMP_FILE=$(mktemp "$CONFIG_DIR/.chordrift.env.XXXXXX")
trap 'rm -f -- "$TEMP_FILE"' EXIT HUP INT TERM
{
    printf 'CHORDRIFT_DATABASE_URL=%s\n' "$DATABASE_URL"
    printf 'CHORDRIFT_PUBLIC_ORIGIN=https://chordrift.suhail.ink/\n'
    printf 'CHORDRIFT_ACCOUNT_ID=%s\n' "$ACCOUNT_ID"
    printf 'CHORDRIFT_BOOTSTRAP_VERIFIED_EMAIL=%s\n' "$BOOTSTRAP_EMAIL"
    printf 'CHORDRIFT_OIDC_ISSUER=https://%s/\n' "$AUTH0_DOMAIN"
    printf 'CHORDRIFT_OIDC_AUTHORIZATION_URL=https://%s/authorize\n' "$AUTH0_DOMAIN"
    printf 'CHORDRIFT_OIDC_TOKEN_URL=https://%s/oauth/token\n' "$AUTH0_DOMAIN"
    printf 'CHORDRIFT_OIDC_USERINFO_URL=https://%s/userinfo\n' "$AUTH0_DOMAIN"
    printf 'CHORDRIFT_OIDC_CLIENT_ID=%s\n' "$AUTH0_CLIENT_ID"
    printf 'CHORDRIFT_OIDC_CLIENT_SECRET=%s\n' "$AUTH0_CLIENT_SECRET"
} >"$TEMP_FILE"
chmod 600 "$TEMP_FILE"
mv "$TEMP_FILE" "$CONFIG_FILE"
trap - EXIT HUP INT TERM
unset AUTH0_CLIENT_SECRET DATABASE_URL

printf 'Wrote %s with mode 0600.\n' "$CONFIG_FILE"
printf 'Back up the Auth0 secret and future Chordrift vault key in 1Password.\n'
