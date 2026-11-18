using QuickLook.Common.Plugin;
using System;
using System.IO;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace QuickLook.Plugin.BullViewer
{
    public sealed class Plugin : IViewer
    {
        private BullDocument document;
        private ContextObject context;
        private Image image;
        private Canvas viewport;
        private MatrixTransform imageTransform;
        private TextBlock status;
        private Button previous, next;
        private int page = -1, generation;
        private double fitScale = 1;
        private bool needsFit, dragging;
        private Point dragStart;
        private Matrix dragMatrix;
        private bool closed;
        public int Priority { get { return 1; } }
        public void Init() { }
        public bool CanHandle(string path)
        {
            if (!string.Equals(Path.GetExtension(path), ".bull", StringComparison.OrdinalIgnoreCase)) return false;
            try
            {
                using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
                {
                    byte[] magic = new byte[8];
                    return file.Read(magic, 0, magic.Length) == 8 && System.Text.Encoding.ASCII.GetString(magic) == "BALOHCOL";
                }
            }
            catch (IOException) { return false; }
            catch (UnauthorizedAccessException) { return false; }
        }
        public void Prepare(string path, ContextObject value) { value.PreferredSize = new Size(900, 720); }
        public async void View(string path, ContextObject value)
        {
            context = value; closed = false; int token = ++generation;
            context.Title = Path.GetFileName(path); context.IsBusy = true;
            var grid = new Grid(); grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); grid.RowDefinitions.Add(new RowDefinition());
            var toolbar = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(8) };
            previous = new Button { Content = "← Previous", Margin = new Thickness(4), Padding = new Thickness(10, 4, 10, 4), IsEnabled = false, Focusable = false };
            next = new Button { Content = "Next →", Margin = new Thickness(4), Padding = new Thickness(10, 4, 10, 4), IsEnabled = false, Focusable = false };
            status = new TextBlock { Text = "Loading…", VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(8) };
            previous.Click += (s, e) => ShowPage(page - 1); next.Click += (s, e) => ShowPage(page + 1);
            toolbar.Children.Add(previous); toolbar.Children.Add(status); toolbar.Children.Add(next); grid.Children.Add(toolbar);
            imageTransform = new MatrixTransform();
            image = new Image { Stretch = Stretch.Fill, RenderTransform = imageTransform, IsHitTestVisible = false };
            viewport = new Canvas { ClipToBounds = true, Background = Brushes.Transparent };
            viewport.Children.Add(image);
            viewport.SizeChanged += (s, e) => { if (needsFit) FitImage(); };
            viewport.PreviewMouseWheel += Zoom;
            viewport.MouseLeftButtonDown += BeginPan;
            viewport.MouseMove += Pan;
            viewport.MouseLeftButtonUp += EndPan;
            viewport.LostMouseCapture += (s, e) => { dragging = false; };
            Grid.SetRow(viewport, 1); grid.Children.Add(viewport); context.ViewerContent = grid;
            BullDocument opened = null;
            try
            {
                opened = await Task.Run(() => BullDocument.Open(path));
                if (closed || token != generation) { opened.Dispose(); return; }
                document = opened; opened = null;
                ShowPage(0);
            }
            catch (Exception ex)
            {
                if (!closed && token == generation) { status.Text = "Cannot open collection: " + ex.Message; context.IsBusy = false; }
            }
            finally { if (opened != null) opened.Dispose(); }
        }
        private async void ShowPage(int index)
        {
            var current = document;
            if (closed || current == null || index < 0 || index >= current.Count) return;
            int token = ++generation;
            previous.IsEnabled = next.IsEnabled = false; status.Text = "Loading page…"; context.IsBusy = true;
            try
            {
                BitmapImage bitmap = await Task.Run(() => {
                    byte[] bytes = current.ReadPage(index);
                    using (var stream = new MemoryStream(bytes, false))
                    {
                        var result = new BitmapImage(); result.BeginInit(); result.CacheOption = BitmapCacheOption.OnLoad;
                        result.StreamSource = stream; result.EndInit(); result.Freeze(); return result;
                    }
                });
                if (closed || token != generation) return;
                // One UI turn commits the new ready image, page identity and transform.
                image.Source = bitmap; image.Width = bitmap.PixelWidth; image.Height = bitmap.PixelHeight;
                page = index; needsFit = true; FitImage();
                status.Text = string.Format("{0} / {1}", index + 1, current.Count);
                previous.IsEnabled = index > 0; next.IsEnabled = index + 1 < current.Count; context.IsBusy = false;
            }
            catch (Exception ex)
            {
                if (!closed && token == generation) { status.Text = "Cannot open page: " + ex.Message; previous.IsEnabled = page > 0; next.IsEnabled = page >= 0 && page + 1 < current.Count; context.IsBusy = false; }
            }
        }
        private void FitImage()
        {
            if (viewport == null || image == null || image.Source == null || viewport.ActualWidth <= 0 || viewport.ActualHeight <= 0) return;
            fitScale = Math.Min(viewport.ActualWidth / image.Width, viewport.ActualHeight / image.Height);
            if (fitScale <= 0 || double.IsNaN(fitScale) || double.IsInfinity(fitScale)) return;
            imageTransform.Matrix = new Matrix(fitScale, 0, 0, fitScale,
                (viewport.ActualWidth - image.Width * fitScale) / 2,
                (viewport.ActualHeight - image.Height * fitScale) / 2);
            needsFit = false;
        }
        private void Zoom(object sender, MouseWheelEventArgs e)
        {
            if (image == null || image.Source == null || needsFit) return;
            Point anchor = (Keyboard.Modifiers & ModifierKeys.Control) != 0
                ? new Point(viewport.ActualWidth / 2, viewport.ActualHeight / 2)
                : e.GetPosition(viewport);
            Matrix matrix = imageTransform.Matrix;
            double scale = Math.Max(fitScale * 0.25, Math.Min(fitScale * 32, matrix.M11 * Math.Pow(1.12, e.Delta / 120.0)));
            double ratio = scale / matrix.M11;
            matrix.OffsetX = anchor.X - (anchor.X - matrix.OffsetX) * ratio;
            matrix.OffsetY = anchor.Y - (anchor.Y - matrix.OffsetY) * ratio;
            matrix.M11 = matrix.M22 = scale;
            imageTransform.Matrix = matrix; e.Handled = true;
        }
        private void BeginPan(object sender, MouseButtonEventArgs e)
        {
            if (image == null || image.Source == null || !viewport.CaptureMouse()) return;
            dragging = true; dragStart = e.GetPosition(viewport); dragMatrix = imageTransform.Matrix; e.Handled = true;
        }
        private void Pan(object sender, MouseEventArgs e)
        {
            if (!dragging) return;
            Point point = e.GetPosition(viewport); Matrix matrix = dragMatrix;
            matrix.OffsetX += point.X - dragStart.X; matrix.OffsetY += point.Y - dragStart.Y;
            imageTransform.Matrix = matrix; e.Handled = true;
        }
        private void EndPan(object sender, MouseButtonEventArgs e)
        {
            if (!dragging) return;
            dragging = false; viewport.ReleaseMouseCapture(); e.Handled = true;
        }
        public void Cleanup()
        {
            closed = true; generation++;
            var old = document; document = null;
            if (old != null) Task.Run(() => old.Dispose());
            if (viewport != null && viewport.IsMouseCaptured) viewport.ReleaseMouseCapture();
            dragging = false; image = null; viewport = null; imageTransform = null; previous = next = null; status = null; context = null;
        }
    }
}
