using NUnit.Framework;
using UnityEngine;
using Core;

namespace Gradiance.UnitTests
{
    public class SimulationManagerTests
    {
        private SimulationManager _simulationManager;
        private GameObject _simManagerGO;

        [SetUp]
        public void SetUp()
        {
            if (SimulationManager.Instance != null)
            {
                Object.DestroyImmediate(SimulationManager.Instance.gameObject);
            }

            _simManagerGO = new GameObject("SimulationManager");
            _simulationManager = _simManagerGO.AddComponent<SimulationManager>();
        }

        [TearDown]
        public void TearDown()
        {
            if (_simManagerGO != null)
            {
                Object.DestroyImmediate(_simManagerGO);
            }
            // Reset Time.timeScale to default
            Time.timeScale = 1.0f;
        }

        [Test]
        public void Singleton_Exists()
        {
            Assert.IsNotNull(SimulationManager.Instance);
            Assert.AreEqual(_simulationManager, SimulationManager.Instance);
        }

        [Test]
        public void TogglePause_ChangesTimeScale()
        {
            // Initial state
            Assert.IsFalse(_simulationManager.IsPaused);
            Assert.AreEqual(1.0f, Time.timeScale);

            // Pause
            _simulationManager.TogglePause();
            Assert.IsTrue(_simulationManager.IsPaused);
            Assert.AreEqual(0f, Time.timeScale);

            // Unpause
            _simulationManager.TogglePause();
            Assert.IsFalse(_simulationManager.IsPaused);
            Assert.AreEqual(1.0f, Time.timeScale);
        }

        [Test]
        public void SetPaused_ExplicitlySetsState()
        {
            _simulationManager.SetPaused(true);
            Assert.IsTrue(_simulationManager.IsPaused);
            Assert.AreEqual(0f, Time.timeScale);

            _simulationManager.SetPaused(false);
            Assert.IsFalse(_simulationManager.IsPaused);
            Assert.AreEqual(1.0f, Time.timeScale);
        }

        [Test]
        public void SetGravity_UpdatesPhysics2D()
        {
            Vector2 newGravity = new Vector2(0, -5.5f);
            _simulationManager.SetGravity(newGravity);

            Assert.AreEqual(newGravity, Physics2D.gravity);
        }
    }
}
