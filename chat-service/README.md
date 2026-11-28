# Project chat setup

Local chat is saved with the .tkv project. Online chat uses Firebase Realtime Database for storage, delivery and presence. A Cloudflare Worker verifies GitHub repository access and signs a short-lived Firebase custom token. This avoids a second account prompt and does not require Firebase Cloud Functions.

1. In a Firebase Spark project, enable Authentication and create a Realtime Database in locked mode. Register a web app. Publish the contents of database.rules.json in the database Rules tab.
2. In the same Google Cloud project create a dedicated service account and generate a JSON key for signing Firebase custom tokens. It needs no database administration role. Keep the downloaded key private.
3. Deploy worker.mjs as a Cloudflare Worker using wrangler.toml. Set the Worker variables PROJECT_ID, DATABASE_URL and API_KEY from Firebase. Set CLIENT_EMAIL from the service account. Store PRIVATE_KEY as a Worker secret, using the decoded multiline PEM value from the service account JSON. Never include the private key in the application or repository.
4. In Tenkivo, open Project chat → Set up chat. Enter the deployed HTTPS Worker URL and a fine-grained GitHub token scoped to the project repository with read-only Metadata access. The URL is remembered on this device for that repository. The chat token is held in memory for the project session. The setup panel also includes the Worker source, database rules and links to each provider.

   For an optional preconfigured distribution, set TENKIVO_CHAT_AUTH_URL to the HTTPS URL of this trusted Worker before compiling the desktop binary. For release builds set the GitHub Actions repository variable of the same name. The native client sends the signed-in user's GitHub credential only to this configured endpoint to verify repository access.
5. Test with two GitHub accounts on the same private repository. Open the project on both devices, send a message, reply and toggle a reaction. Verify presence disappears after disconnection. Reopen the project to verify history. Test another repository to verify isolation. Room access is rechecked every 12 minutes and grants expire after 15 minutes.

The latest 50 messages load first. Earlier messages can be loaded on demand. Read-only repository members may participate in the chat independently of canvas editing permissions.
