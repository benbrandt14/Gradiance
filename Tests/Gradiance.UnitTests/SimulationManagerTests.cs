using NUnit.Framework;
using UnityEngine;
using Core;
using System.Reflection;

namespace Gradiance.UnitTests
{
    public class SimulationManagerTests
    {
        private SimulationManager _simulationManager;
        private GameObject _simManagerGO;

        [SetUp]
        public void SetUp()
        {
            ResetSingleton();

            _simManagerGO = new GameObject("SimulationManager");
            _simulationManager = _simManagerGO.AddComponent<SimulationManager>();
        }

        [TearDown]
        public void TearDown()
        {
            if (_simManagerGO != null)
            {
                UnityEngine.Object.DestroyImmediate(_simManagerGO);
            }
            // Reset Time.timeScale to default
            Time.timeScale = 1.0f;
            ResetSingleton();
        }

        private void ResetSingleton()
        {
             if (SimulationManager.Instance != null)
            {
                var instanceProp = typeof(SimulationManager).GetProperty("Instance", BindingFlags.Static | BindingFlags.Public);
                if (instanceProp != null)
                {
                    instanceProp.SetValue(null, null);
                }
            }
        }

        [Test]
        public void Singleton_Exists()
        {
            Assert.That(SimulationManager.Instance, Is.Not.Null);
            Assert.That(SimulationManager.Instance, Is.EqualTo(_simulationManager));
        }

        [Test]
        public void TogglePause_ChangesTimeScale()
        {
            // Initial state
            Assert.That(_simulationManager.IsPaused, Is.False);
            Assert.That(Time.timeScale, Is.EqualTo(1.0f));

            // Pause
            _simulationManager.TogglePause();
            Assert.That(_simulationManager.IsPaused, Is.True);
            Assert.That(Time.timeScale, Is.EqualTo(0f));

            // Unpause
            _simulationManager.TogglePause();
            Assert.That(_simulationManager.IsPaused, Is.False);
            Assert.That(Time.timeScale, Is.EqualTo(1.0f));
        }

        [Test]
        public void SetPaused_ExplicitlySetsState()
        {
            _simulationManager.SetPaused(true);
            Assert.That(_simulationManager.IsPaused, Is.True);
            Assert.That(Time.timeScale, Is.EqualTo(0f));

            _simulationManager.SetPaused(false);
            Assert.That(_simulationManager.IsPaused, Is.False);
            Assert.That(Time.timeScale, Is.EqualTo(1.0f));
        }

        [Test]
        public void SetGravity_UpdatesPhysics2D()
        {
            Vector2 newGravity = new Vector2(0, -5.5f);
            _simulationManager.SetGravity(newGravity);

            Assert.That(Physics2D.gravity, Is.EqualTo(newGravity));
        }
    }
}
