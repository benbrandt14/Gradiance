using System;
using System.Reflection;
using NUnit.Framework;
using UnityEngine;

namespace Gradiance.UnitTests
{
    [SetUpFixture]
    public class TestBootstrapper
    {
        [OneTimeSetUp]
        public void RunBeforeAnyTests()
        {
            // Simulate Unity's RuntimeInitializeOnLoadMethod
            // In a real Unity build, the engine does this. In our Mock Engine, we must do it manually.

            // Find all methods in the assembly that have the RuntimeInitializeOnLoadMethod attribute
            // For now, we know specifically about SceneInitializer, so we can target it directly for simplicity,
            // or do a broader search if we want to be more generic. Let's start with targeted.
            var assembly = typeof(Core.SceneInitializer).Assembly;
            var type = assembly.GetType("Core.SceneInitializer");

            if (type != null)
            {
                var method = type.GetMethod("InitializeScene", BindingFlags.Static | BindingFlags.NonPublic);
                if (method != null)
                {
                    Debug.Log("Bootstrapper: Invoking SceneInitializer.InitializeScene...");
                    method.Invoke(null, null);
                }
                else
                {
                    Debug.LogError("Bootstrapper: Could not find InitializeScene method.");
                }
            }
            else
            {
                Debug.LogError("Bootstrapper: Could not find SceneInitializer type.");
            }
        }
    }
}
