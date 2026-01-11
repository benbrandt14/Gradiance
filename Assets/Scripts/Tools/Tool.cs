using UnityEngine;

namespace Tools
{
    public abstract class Tool : MonoBehaviour
    {
        public abstract string ToolName { get; }

        public virtual void OnToolSelected()
        {
        }

        public virtual void OnToolDeselected()
        {
        }

        public abstract void OnToolUpdate();
    }
}
