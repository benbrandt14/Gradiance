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
            Assert.AreEqual(4, c.x);
            Assert.AreEqual(6, c.y);
        }

        [Test]
        public void ToolManagerSingletonExists()
        {
            // Just verifying we can access the types without crashing
            var tm = ToolManager.Instance;
            Assert.IsNotNull(tm);
        }
    }
}
