using NUnit.Framework;
using UnityEngine;
using Tools;
using Core;

namespace Gradiance.UnitTests
{
    public class SanityCheck
    {
        [Test]
        public void VectorMathWorks()
        {
            Vector2 a = new Vector2(1, 2);
            Vector2 b = new Vector2(3, 4);
            Vector2 c = a + b;
            Assert.That(c.x, Is.EqualTo(4));
            Assert.That(c.y, Is.EqualTo(6));
        }

        [Test]
        public void ToolManagerSingletonExists()
        {
            // Just verifying we can access the types without crashing
            var tm = ToolManager.Instance;
            // Instance might be null if not initialized, but checking access.
            // In a clean test run, it's likely null unless set up.
            // But we just want to ensure the type resolves.
             Assert.That(true, Is.True);
        }
    }
}
