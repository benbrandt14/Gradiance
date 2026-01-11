using Core;
using NUnit.Framework;
using Tools;
using UnityEngine;

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
            Assert.That(tm, Is.Not.Null);
        }
    }
}
