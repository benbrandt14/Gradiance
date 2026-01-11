#!/bin/bash
set -e

# Update package lists
apt-get update

# Install dependencies for .NET (often needed on minimal images)
apt-get install -y wget apt-transport-https software-properties-common

# Download Microsoft signing key and repository
wget https://packages.microsoft.com/config/ubuntu/20.04/packages-microsoft-prod.deb -O packages-microsoft-prod.deb
dpkg -i packages-microsoft-prod.deb
rm packages-microsoft-prod.deb

# Update repositories again
apt-get update

# Install .NET SDK 8.0
apt-get install -y dotnet-sdk-8.0

echo "Setup complete. Dotnet version:"
dotnet --version
