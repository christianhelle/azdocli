# Azure DevOps CLI

CLI tool for interacting with Azure DevOps.

## Features

* **Repository Management**: List, create, delete, clone, view, and manage pull requests
* **Pipeline Management**: Manage Azure DevOps pipelines  
* **Board Management**: Manage Azure DevOps boards and work items
* **Authentication**: Secure login using Personal Access Tokens (PAT)
* **Default Project**: Set a default project to avoid specifying --project for every command

## Key Capabilities

### Repository Operations
* List and manage repositories
* Clone all repositories from a project with parallel execution
* Create, view, and manage pull requests
* Delete repositories (soft/hard delete options)

### Pipeline Operations  
* List all pipelines in a project
* View pipeline runs and build details
* Start new pipeline runs
* Real-time build status updates

### Board Operations
* View and manage work items (bugs, tasks, user stories, features, epics)
* Create, update, and delete work items
* Open work items in web browser
* Full CRUD operations support

## Authentication Setup

Create a Personal Access Token (PAT) in Azure DevOps:

1. Sign in to https://dev.azure.com/{yourorganization}
2. Click profile picture > **Personal Access Tokens**
3. Click **+ New Token**
4. Configure scopes:
   * **Code**: Read & write
   * **Build**: Read & execute  
   * **Work Items**: Read & write
   * **Project and Team**: Read
5. Copy and securely store the token

## Basic Usage

   # Login with your PAT
   azdocli login
   
   # Set default project (optional)
   azdocli project MyProject
   
   # Repository management
   azdocli repos list
   azdocli repos clone --parallel
   azdocli repos pr list --repo MyRepo
   
   # Pipeline management
   azdocli pipelines list
   azdocli pipelines run --id 42
   
   # Board management
   azdocli boards work-item show --id 123
   azdocli boards work-item create bug --title "Fix login"

## Commands

   USAGE: azdocli [SUBCOMMAND]
   
   SUBCOMMANDS:
   boards       Manage Azure DevOps boards
   login        Login with Personal Access Token
   logout       Logout from Azure DevOps
   pipelines    Manage Azure DevOps pipelines  
   project      Set/view default project
   repos        Manage Azure DevOps repositories
   
   OPTIONS:
   -h, --help       Print help
   -V, --version    Print version

For complete documentation and advanced usage examples, visit the project repository.