using NUnit.Framework;
using UnityEngine;
using Tools;
using UI;
using System.Reflection;

namespace Tests.Tools
{
    public class MoveToolTests
    {
        private MoveTool _moveTool;
        private GameObject _testObject;
        private Rigidbody2D _rb;

        [SetUp]
        public void Setup()
        {
            _moveTool = new MoveTool();
            _testObject = new GameObject("TestObject");
            _rb = _testObject.AddComponent<Rigidbody2D>();
            var col = _testObject.AddComponent<BoxCollider2D>();

            // Mock camera for ScreenToWorldPoint if needed, or rely on MockEngine's default behavior
            // The MockEngine usually needs a Camera.main
            if (Camera.main == null)
            {
                var camGo = new GameObject("MainCamera");
                var cam = camGo.AddComponent<Camera>();
                camGo.tag = "MainCamera";
            }
        }

        [TearDown]
        public void TearDown()
        {
            if (_testObject != null) Object.DestroyImmediate(_testObject);
            if (_moveTool != null) _moveTool.OnToolDeselected(); // Cleanup halo
        }

        [Test]
        public void Test_MoveTool_Initialization()
        {
            // Act
            _moveTool.OnToolSelected();

            // Assert
            // Use Reflection to check private field _selectionHalo
            var field = typeof(MoveTool).GetField("_selectionHalo", BindingFlags.NonPublic | BindingFlags.Instance);
            var halo = field.GetValue(_moveTool) as GameObject;

            Assert.That(halo, Is.Not.Null);
            Assert.That(halo.name, Is.EqualTo("SelectionHalo"));
        }

        [Test]
        public void Test_MoveTool_Deselection_Cleanup()
        {
            // Arrange
            _moveTool.OnToolSelected();

            // Act
            _moveTool.OnToolDeselected();

            // Assert
            var field = typeof(MoveTool).GetField("_selectionHalo", BindingFlags.NonPublic | BindingFlags.Instance);
            var halo = field.GetValue(_moveTool) as GameObject;

            // In MockEngine, Destroy might be immediate or delayed. If immediate, it should be null or "null-like"
            // Unity overrides == null, but reflection GetValue returns the object.
            // We check if it equals null (Unity check)
            Assert.That(halo == null, Is.True);
        }
    }
}
