.PHONY: setup build test format lint

setup:
	sudo bash setup.sh

build:
	dotnet build Tests/MockUnityEngine/Gradiance.MockEngine.csproj
	dotnet build Tests/Gradiance.UnitTests/Gradiance.UnitTests.csproj

test:
	dotnet test Tests/Gradiance.UnitTests/Gradiance.UnitTests.csproj

format:
	dotnet format Tests/Gradiance.UnitTests/Gradiance.UnitTests.csproj
	dotnet format Tests/MockUnityEngine/Gradiance.MockEngine.csproj

lint:
	dotnet format --verify-no-changes Tests/Gradiance.UnitTests/Gradiance.UnitTests.csproj
