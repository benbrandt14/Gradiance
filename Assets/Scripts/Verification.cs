using UnityEngine;
using Core;
using Tools;
using UI;
using Physics;

public class Verification : MonoBehaviour
{
    // This script doesn't run, but ensures everything compiles and references are valid
    void TestReferences()
    {
        var sm = SimulationManager.Instance;
        var tm = ToolManager.Instance;
        var um = UIManager.Instance;

        Tool t = new MoveTool();
        Tool b = new BoxTool();
        Tool c = new CircleTool();
        Tool h = new HingeTool();
        Tool s = new SpringTool();

        var box = PhysicsObjectFactory.CreateBox(Vector2.zero, Vector2.one);
        var circ = PhysicsObjectFactory.CreateCircle(Vector2.zero, 1f);
    }
}
