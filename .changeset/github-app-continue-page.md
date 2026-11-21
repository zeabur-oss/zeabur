---
default: major
---

# The step that sends the GitHub App to GitHub uses the web interface

The page between Zeabur OSS's form and GitHub's confirmation is now part of the web interface, at `/cluster/github-app/continue`, with the same look as the rest of Zeabur OSS in light and dark and in Zeabur Desktop. It shows the App's name and the account it goes to. Only this page may send forms to GitHub. `GET /api/v1/cluster/github-app/form` now answers JSON with the form's address and the manifest instead of an HTML page.
