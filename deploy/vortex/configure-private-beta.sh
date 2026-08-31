#!/bin/sh

# Materialize the Vortex runtime environment from durable 1Password items.
# Run this on an authenticated operator workstation; Chordrift and Vortex do
# not depend on Apogee or a continuously available 1Password session.

set -eu
umask 077

TARGET_HOST=${1:-vortex}
AUTH0_ITEM=${CHORDRIFT_AUTH0_OP_ITEM:-jh3ya32enwcjdy5myvlyrhfhjm}
NEON_ITEM=${CHORDRIFT_NEON_OP_ITEM:-igyk24uav5igf7r7m5i6zv4vai}
SPOTIFY_ITEM=${CHORDRIFT_SPOTIFY_OP_ITEM:-mmguucamllakhav2uniw3uixci}
PROVIDER_VAULT_ITEM=${CHORDRIFT_PROVIDER_VAULT_OP_ITEM:-j7abwton3e2zmhf5jnwc2idyha}
ACCOUNT_ID=669e8896-0ba1-f730-67cf-351467f4d6eb
REMOTE_DIR=.config/chordrift-hosted
REMOTE_FILE=$REMOTE_DIR/chordrift.env

for command_name in op jq ssh scp; do
    command -v "$command_name" >/dev/null 2>&1 || {
        printf 'Required command is unavailable: %s\n' "$command_name" >&2
        exit 2
    }
done

AUTH0_DOMAIN=$(op read "op://API/$AUTH0_ITEM/domain")
AUTH0_CLIENT_ID=$(op read "op://API/$AUTH0_ITEM/username")
AUTH0_CLIENT_SECRET=$(op read "op://API/$AUTH0_ITEM/credential")
BOOTSTRAP_EMAIL=$(op read "op://API/$AUTH0_ITEM/bootstrap_email")
DATABASE_HOST=$(op read "op://API/$NEON_ITEM/server")
DATABASE_PORT=$(op read "op://API/$NEON_ITEM/port")
DATABASE_NAME=$(op read "op://API/$NEON_ITEM/database")
DATABASE_USER=$(op read "op://API/$NEON_ITEM/username")
DATABASE_PASSWORD=$(op read "op://API/$NEON_ITEM/password")
DATABASE_OPTIONS=$(op read "op://API/$NEON_ITEM/connection options")
SPOTIFY_CLIENT_ID=$(op read "op://API/$SPOTIFY_ITEM/username")
PROVIDER_VAULT_KEY_ID=$(op read "op://API/$PROVIDER_VAULT_ITEM/username")
PROVIDER_VAULT_KEY_B64=$(op read "op://API/$PROVIDER_VAULT_ITEM/credential")

case "$BOOTSTRAP_EMAIL" in *@*) ;; *) printf 'A valid bootstrap email is required.\n' >&2; exit 2 ;; esac
case "$AUTH0_DOMAIN" in https://*|http://*|*/*) printf '1Password Auth0 domain must be a hostname.\n' >&2; exit 2 ;; esac
case "$DATABASE_PORT" in *[!0-9]*|'') printf '1Password database port is invalid.\n' >&2; exit 2 ;; esac

export CHORDRIFT_DATABASE_PASSWORD_FOR_ENCODING=$DATABASE_PASSWORD
ENCODED_DATABASE_PASSWORD=$(jq -nr 'env.CHORDRIFT_DATABASE_PASSWORD_FOR_ENCODING | @uri')
unset CHORDRIFT_DATABASE_PASSWORD_FOR_ENCODING DATABASE_PASSWORD
DATABASE_URL="postgresql://$DATABASE_USER:$ENCODED_DATABASE_PASSWORD@$DATABASE_HOST:$DATABASE_PORT/$DATABASE_NAME?$DATABASE_OPTIONS"

TEMP_FILE=$(mktemp "${TMPDIR:-/tmp}/chordrift-hosted.XXXXXX")
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
    printf 'CHORDRIFT_SPOTIFY_CLIENT_ID=%s\n' "$SPOTIFY_CLIENT_ID"
    printf 'CHORDRIFT_PROVIDER_VAULT_ACTIVE_KEY_ID=%s\n' "$PROVIDER_VAULT_KEY_ID"
    printf 'CHORDRIFT_PROVIDER_VAULT_KEY_B64=%s\n' "$PROVIDER_VAULT_KEY_B64"
} >"$TEMP_FILE"
chmod 600 "$TEMP_FILE"
unset AUTH0_CLIENT_SECRET DATABASE_URL ENCODED_DATABASE_PASSWORD PROVIDER_VAULT_KEY_B64

ssh "$TARGET_HOST" "mkdir -p '$REMOTE_DIR' && chmod 700 '$REMOTE_DIR'"
scp -q "$TEMP_FILE" "$TARGET_HOST:$REMOTE_FILE.next"
ssh "$TARGET_HOST" "chmod 600 '$REMOTE_FILE.next' && mv '$REMOTE_FILE.next' '$REMOTE_FILE' && stat -c '%a %U:%G %n' '$REMOTE_FILE'"

printf 'Materialized Chordrift runtime configuration from 1Password on %s.\n' "$TARGET_HOST"
