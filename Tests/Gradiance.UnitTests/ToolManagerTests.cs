using NUnit.Framework;
using UnityEngine;
using Tools;
using System.Linq;

namespace Gradiance.UnitTests
{
    public class ToolManagerTests
    {
        private class MockTool : Tool
        {
            public override string ToolName { get; }
            public bool IsSelected { get; private set; }
            public bool IsDeselected { get; private set; }
            public int UpdateCount { get; private set; }

            public MockTool(string name)
            {
                ToolName = name;
            }

            public override void OnToolSelected()
            {
                IsSelected = true;
                IsDeselected = false;
            }

            public override void OnToolDeselected()
            {
                IsSelected = false;
                IsDeselected = true;
            }

            public override void OnToolUpdate()
            {
                UpdateCount++;
            }
        }

        private ToolManager _toolManager;
        private GameObject _toolManagerGO;

        [SetUp]
        public void SetUp()
        {
            // Reset singleton if possible or create new GO
            // Since ToolManager is a Singleton, we might need to be careful.
            // In a real Unity environment, we'd destroy the old one.
            // In mock, we can just create a new one, but the static Instance might persist.

            // Checking if Instance exists and destroying it if so (simulating scene reload)
            if (ToolManager.Instance != null)
            {
                Object.DestroyImmediate(ToolManager.Instance.gameObject);
            }

            _toolManagerGO = new GameObject("ToolManager");
            _toolManager = _toolManagerGO.AddComponent<ToolManager>();
            // Awake is called by AddComponent in the Mock engine usually,
            // but let's double check AGENTS.md.
            // "MockTypes.AddComponent<T> reflectively invokes Awake() on MonoBehaviours."
            // So Instance should be set.
        }

        [TearDown]
        public void TearDown()
        {
            if (_toolManagerGO != null)
            {
                Object.DestroyImmediate(_toolManagerGO);
            }
        }

        [Test]
        public void RegisterTool_AddsToolToList()
        {
            var tool = new MockTool("TestTool");
            _toolManager.RegisterTool(tool);

            var retrievedTool = _toolManager.GetTool<MockTool>();
            Assert.IsNotNull(retrievedTool);
            Assert.AreEqual(tool, retrievedTool);
        }

        [Test]
        public void SelectTool_UpdatesCurrentTool()
        {
            var tool = new MockTool("TestTool");
            _toolManager.RegisterTool(tool);
            _toolManager.SelectTool(tool);

            Assert.AreEqual(tool, _toolManager.CurrentTool);
        }

        [Test]
        public void SelectTool_TriggersCallbacks()
        {
            var tool1 = new MockTool("Tool1");
            var tool2 = new MockTool("Tool2");
            _toolManager.RegisterTool(tool1);
            _toolManager.RegisterTool(tool2);

            _toolManager.SelectTool(tool1);
            Assert.IsTrue(tool1.IsSelected);
            Assert.IsFalse(tool1.IsDeselected);

            _toolManager.SelectTool(tool2);
            Assert.IsFalse(tool1.IsSelected);
            Assert.IsTrue(tool1.IsDeselected);
            Assert.IsTrue(tool2.IsSelected);
        }

        [Test]
        public void SelectToolByName_SelectsCorrectTool()
        {
            var tool = new MockTool("NamedTool");
            _toolManager.RegisterTool(tool);

            _toolManager.SelectTool("NamedTool");
            Assert.AreEqual(tool, _toolManager.CurrentTool);
        }

        [Test]
        public void SelectToolByName_LogsWarningIfNotFound()
        {
            // Since we can't easily assert on Debug.Log in this setup without a log assert,
            // we primarily check that CurrentTool remains null or unchanged.
            _toolManager.SelectTool("NonExistentTool");
            Assert.IsNull(_toolManager.CurrentTool);
        }
    }
}
