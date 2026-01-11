using NUnit.Framework;
using UnityEngine;
using Tools;
using System.Linq;
using System.Reflection;

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
            ResetSingleton();

            _toolManagerGO = new GameObject("ToolManager");
            _toolManager = _toolManagerGO.AddComponent<ToolManager>();
        }

        [TearDown]
        public void TearDown()
        {
            if (_toolManagerGO != null)
            {
                UnityEngine.Object.DestroyImmediate(_toolManagerGO);
            }
            ResetSingleton();
        }

        private void ResetSingleton()
        {
            // Reset singleton via Reflection because Mock Engine doesn't handle static lifecycle
            if (ToolManager.Instance != null)
            {
                var instanceProp = typeof(ToolManager).GetProperty("Instance", BindingFlags.Static | BindingFlags.Public);
                if (instanceProp != null)
                {
                    instanceProp.SetValue(null, null);
                }
            }
        }

        [Test]
        public void RegisterTool_AddsToolToList()
        {
            var tool = new MockTool("TestTool");
            _toolManager.RegisterTool(tool);

            var retrievedTool = _toolManager.GetTool<MockTool>();
            Assert.That(retrievedTool, Is.Not.Null);
            Assert.That(retrievedTool, Is.EqualTo(tool));
        }

        [Test]
        public void SelectTool_UpdatesCurrentTool()
        {
            var tool = new MockTool("TestTool");
            _toolManager.RegisterTool(tool);
            _toolManager.SelectTool(tool);

            Assert.That(_toolManager.CurrentTool, Is.EqualTo(tool));
        }

        [Test]
        public void SelectTool_TriggersCallbacks()
        {
            var tool1 = new MockTool("Tool1");
            var tool2 = new MockTool("Tool2");
            _toolManager.RegisterTool(tool1);
            _toolManager.RegisterTool(tool2);

            _toolManager.SelectTool(tool1);
            Assert.That(tool1.IsSelected, Is.True);
            Assert.That(tool1.IsDeselected, Is.False);

            _toolManager.SelectTool(tool2);
            Assert.That(tool1.IsSelected, Is.False);
            Assert.That(tool1.IsDeselected, Is.True);
            Assert.That(tool2.IsSelected, Is.True);
        }

        [Test]
        public void SelectToolByName_SelectsCorrectTool()
        {
            var tool = new MockTool("NamedTool");
            _toolManager.RegisterTool(tool);

            _toolManager.SelectTool("NamedTool");
            Assert.That(_toolManager.CurrentTool, Is.EqualTo(tool));
        }

        [Test]
        public void SelectToolByName_LogsWarningIfNotFound()
        {
            _toolManager.SelectTool("NonExistentTool");
            Assert.That(_toolManager.CurrentTool, Is.Null);
        }
    }
}
