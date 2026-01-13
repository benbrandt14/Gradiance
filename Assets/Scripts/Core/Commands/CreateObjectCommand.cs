using UnityEngine;

namespace Core.Commands
{
    public class CreateObjectCommand : ICommand
    {
        private readonly GameObject _createdObject;

        // We might want to store state to recreate it if we destroyed it,
        // but typically 'Undo' just disables/hides it, or destroys it?
        // If we destroy it, we need to be able to recreate it exactly.
        // For simplicity: Undo -> Disable/Deactivate. Redo -> Activate.
        // If 'Delete' command exists, it would Deactivate on Execute, and Activate on Undo.
        public CreateObjectCommand(GameObject createdObject)
        {
            _createdObject = createdObject;
        }

        public void Execute()
        {
            if (_createdObject != null)
            {
                _createdObject.SetActive(true);
            }
        }

        public void Undo()
        {
            if (_createdObject != null)
            {
                _createdObject.SetActive(false);
            }
        }
    }
}
